/* Fixed own-process image observations. A mapped vnode and loaded LC_UUID bind
 * backing-object identity, not a cryptographic digest of resident code pages.
 * No path alone is accepted as the identity of an already loaded image. */
#include "runtime.h"
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>

static int failure(dp_runtime_error *error, const char *code, const char *format, ...) {
    if (error) {
        snprintf(error->code, sizeof(error->code), "%s", code);
        va_list arguments;
        va_start(arguments, format);
        vsnprintf(error->message, sizeof(error->message), format, arguments);
        va_end(arguments);
    }
    return 0;
}

#ifdef __APPLE__
#include <crt_externs.h>
#include <dlfcn.h>
#include <errno.h>
#include <libproc.h>
#include <mach/mach.h>
#include <mach/mach_vm.h>
#include <mach-o/loader.h>
#include <stdlib.h>
#include <sys/proc_info.h>
#include <sys/stat.h>
#include <sys/sysctl.h>
#include <unistd.h>
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>

#define MAX_COMMAND_BYTES (1024U * 1024U)
#define MAX_COMMANDS 4096U
_Static_assert(sizeof(((struct vnode_info_path *)0)->vip_path) == DP_RUNTIME_PATH,
    "mapped vnode path bound differs from the fixed ABI");

static int region(uint64_t address, struct proc_regionwithpathinfo *out, dp_runtime_error *error) {
    memset(out, 0, sizeof(*out));
    errno = 0;
    int result = proc_pidinfo(getpid(), PROC_PIDREGIONPATHINFO, address, out, sizeof(*out));
    if (result != (int)sizeof(*out))
        return failure(error, "mapping_unavailable", "own-process mapped vnode observation failed (%d, errno %d)", result, errno);
    const struct proc_regioninfo *map = &out->prp_prinfo;
    const struct vinfo_stat *stat = &out->prp_vip.vip_vi.vi_stat;
    if (address < map->pri_address || address - map->pri_address >= map->pri_size ||
        !(map->pri_protection & VM_PROT_READ) || map->pri_protection & VM_PROT_WRITE ||
        !S_ISREG(stat->vst_mode) || !stat->vst_dev || !stat->vst_ino || stat->vst_size <= 0)
        return failure(error, "mapping_unsupported", "image address has no stable read-only regular-file mapping");
    if (!memchr(out->prp_vip.vip_path, 0, sizeof(out->prp_vip.vip_path)) || out->prp_vip.vip_path[0] != '/')
        return failure(error, "mapping_unsupported", "mapped vnode has no complete absolute reopening path");
    return 1;
}

static int same_vnode(const struct vinfo_stat *a, const struct vinfo_stat *b) {
    return a->vst_dev == b->vst_dev && a->vst_ino == b->vst_ino &&
        a->vst_size == b->vst_size && a->vst_gen == b->vst_gen &&
        a->vst_mtime == b->vst_mtime && a->vst_mtimensec == b->vst_mtimensec &&
        a->vst_ctime == b->vst_ctime && a->vst_ctimensec == b->vst_ctimensec &&
        a->vst_birthtime == b->vst_birthtime && a->vst_birthtimensec == b->vst_birthtimensec;
}

static int memory_read(uint64_t address, void *output, size_t size, dp_runtime_error *error) {
    mach_vm_size_t copied = 0;
    kern_return_t result = mach_vm_read_overwrite(mach_task_self(), address, size,
        (mach_vm_address_t)(uintptr_t)output, &copied);
    if (result != KERN_SUCCESS || copied != size)
        return failure(error, "mapping_changed", "bounded loaded-image read failed (%d)", result);
    return 1;
}

static int file_read(int fd, uint64_t offset, void *output, size_t size, dp_runtime_error *error) {
    if (offset > INT64_MAX || size > INT64_MAX - offset)
        return failure(error, "image_invalid", "image header offset exceeds descriptor bounds");
    size_t completed = 0;
    unsigned interruptions = 0;
    while (completed < size) {
        ssize_t count = pread(fd, (char *)output + completed, size - completed, (off_t)(offset + completed));
        if (count < 0 && errno == EINTR && interruptions++ < 8) continue;
        if (count <= 0)
            return failure(error, "image_io", "bounded image header read failed (errno %d)", count < 0 ? errno : 0);
        completed += (size_t)count;
    }
    return 1;
}

static int header_valid(const struct mach_header_64 *header, uint32_t kind, dp_runtime_error *error) {
    if (header->magic != MH_MAGIC_64 || header->reserved ||
        header->filetype != (kind == 0 ? MH_EXECUTE : MH_DYLIB) ||
        header->flags & MH_DYLIB_IN_CACHE || !header->ncmds || header->ncmds > MAX_COMMANDS ||
        !header->sizeofcmds || header->sizeofcmds > MAX_COMMAND_BYTES)
        return failure(error, "image_unsupported", "runtime identity requires a bounded standalone 64-bit Mach-O image");
    return 1;
}

static int command_uuid(const unsigned char *bytes, const struct mach_header_64 *header,
                        uint8_t uuid[16], dp_runtime_error *error) {
    size_t offset = 0;
    unsigned found = 0;
    for (uint32_t index = 0; index < header->ncmds; ++index) {
        if (header->sizeofcmds - offset < sizeof(struct load_command))
            return failure(error, "image_invalid", "truncated Mach-O load command");
        struct load_command command;
        memcpy(&command, bytes + offset, sizeof(command));
        if (command.cmdsize < sizeof(command) || command.cmdsize % 8 || command.cmdsize > header->sizeofcmds - offset)
            return failure(error, "image_invalid", "Mach-O load command exceeds its bounded header");
        if (command.cmd == LC_UUID) {
            if (++found != 1 || command.cmdsize != sizeof(struct uuid_command))
                return failure(error, "image_invalid", "Mach-O UUID command is ambiguous");
            memcpy(uuid, bytes + offset + sizeof(command), 16);
        }
        offset += command.cmdsize;
    }
    unsigned nonzero = 0;
    for (unsigned i = 0; i < 16; ++i) nonzero |= uuid[i];
    if (offset != header->sizeofcmds || found != 1 || !nonzero)
        return failure(error, "image_invalid", "Mach-O image has no exact nonzero UUID");
    return 1;
}

static int copy_stat(dp_runtime_image *image, const struct vinfo_stat *stat, dp_runtime_error *error) {
    if (stat->vst_mtimensec < 0 || stat->vst_mtimensec >= 1000000000 ||
        stat->vst_ctimensec < 0 || stat->vst_ctimensec >= 1000000000 ||
        stat->vst_birthtimensec < 0 || stat->vst_birthtimensec >= 1000000000)
        return failure(error, "image_invalid", "mapped vnode timestamps exceed nanosecond bounds");
    image->device = stat->vst_dev; image->inode = stat->vst_ino; image->file_size = (uint64_t)stat->vst_size;
    image->modification_seconds = stat->vst_mtime; image->modification_nanoseconds = (uint32_t)stat->vst_mtimensec;
    image->change_seconds = stat->vst_ctime; image->change_nanoseconds = (uint32_t)stat->vst_ctimensec;
    image->birth_seconds = stat->vst_birthtime; image->birth_nanoseconds = (uint32_t)stat->vst_birthtimensec;
    image->generation = stat->vst_gen;
    return 1;
}

int dp_runtime_capture(uint32_t kind, dp_runtime_image *image, dp_runtime_error *error) {
    if (!image || kind >= DP_RUNTIME_ROLES) return failure(error, "image_invalid", "unknown runtime image role");
    memset(image, 0, sizeof(*image));
    const void *anchor;
    switch (kind) {
        case 0: anchor = _NSGetMachExecuteHeader(); break;
        case 1: anchor = (const void *)&avcodec_version; break;
        case 2: anchor = (const void *)&avformat_version; break;
        case 3: anchor = (const void *)&avutil_version; break;
        case 4: anchor = dlsym(RTLD_DEFAULT, "swscale_version"); break;
        default: anchor = dlsym(RTLD_DEFAULT, "avfilter_version"); break;
    }
    Dl_info location;
    memset(&location, 0, sizeof(location));
    if (!anchor || !dladdr(anchor, &location) || !location.dli_fbase)
        return failure(error, "image_unavailable", "required loaded runtime image is unavailable");
    uint64_t base = (uint64_t)(uintptr_t)location.dli_fbase;
    uint64_t symbol = (uint64_t)(uintptr_t)anchor;
    if (kind == 0 && base != symbol)
        return failure(error, "image_invalid", "helper header differs from its loaded image base");
    struct proc_regionwithpathinfo mapped, code, after;
    if (!region(base, &mapped, error) || !region(symbol, &code, error)) return 0;
    if (!same_vnode(&mapped.prp_vip.vip_vi.vi_stat, &code.prp_vip.vip_vi.vi_stat))
        return failure(error, "mapping_changed", "loaded symbol and header have different mapped backing objects");
    const struct proc_regioninfo *map = &mapped.prp_prinfo;
    uint64_t delta = base - map->pri_address;
    if (map->pri_offset > UINT64_MAX - delta)
        return failure(error, "image_invalid", "mapped header offset overflow");
    uint64_t file_offset = map->pri_offset + delta;
    struct mach_header_64 header;
    if (sizeof(header) > map->pri_size - delta)
        return failure(error, "image_invalid", "loaded Mach-O header exceeds its readable region");
    if (!memory_read(base, &header, sizeof(header), error) || !header_valid(&header, kind, error)) return 0;
    uint64_t total = sizeof(header) + (uint64_t)header.sizeofcmds;
    if (base > UINT64_MAX - total || total > map->pri_size - delta || file_offset > (uint64_t)mapped.prp_vip.vip_vi.vi_stat.vst_size ||
        total > (uint64_t)mapped.prp_vip.vip_vi.vi_stat.vst_size - file_offset)
        return failure(error, "image_invalid", "loaded Mach-O header exceeds its mapped backing object");
    unsigned char *commands = malloc(header.sizeofcmds);
    if (!commands) return failure(error, "image_capacity", "allocate bounded Mach-O header observation");
    int valid = memory_read(base + sizeof(header), commands, header.sizeofcmds, error) &&
        command_uuid(commands, &header, image->uuid, error);
    free(commands);
    if (!valid || !region(base, &after, error)) return 0;
    if (!same_vnode(&mapped.prp_vip.vip_vi.vi_stat, &after.prp_vip.vip_vi.vi_stat) ||
        map->pri_address != after.prp_prinfo.pri_address || map->pri_size != after.prp_prinfo.pri_size ||
        map->pri_offset != after.prp_prinfo.pri_offset)
        return failure(error, "mapping_changed", "loaded image changed during observation");
    image->abi_version = DP_RUNTIME_ABI; image->kind = kind;
    image->cpu_type = (uint32_t)header.cputype; image->cpu_subtype = (uint32_t)header.cpusubtype;
    image->file_type = header.filetype;
    image->header_address = base; image->anchor_address = symbol; image->header_file_offset = file_offset;
    memcpy(image->path, mapped.prp_vip.vip_path, strlen(mapped.prp_vip.vip_path) + 1);
    return copy_stat(image, &mapped.prp_vip.vip_vi.vi_stat, error);
}

static int same_image(const dp_runtime_image *a, const dp_runtime_image *b) {
    return a->abi_version == b->abi_version && a->kind == b->kind &&
        a->device == b->device && a->inode == b->inode && a->file_size == b->file_size &&
        a->generation == b->generation && !memcmp(a->uuid, b->uuid, 16) &&
        a->modification_seconds == b->modification_seconds && a->modification_nanoseconds == b->modification_nanoseconds &&
        a->change_seconds == b->change_seconds && a->change_nanoseconds == b->change_nanoseconds &&
        a->birth_seconds == b->birth_seconds && a->birth_nanoseconds == b->birth_nanoseconds &&
        a->cpu_type == b->cpu_type && a->cpu_subtype == b->cpu_subtype && a->file_type == b->file_type &&
        a->header_address == b->header_address && a->anchor_address == b->anchor_address &&
        a->header_file_offset == b->header_file_offset;
}

static int descriptor_matches(const dp_runtime_image *image, int fd, dp_runtime_error *error) {
    struct stat stat;
    if (fstat(fd, &stat)) return failure(error, "image_io", "stat pinned runtime descriptor failed (errno %d)", errno);
    if (!S_ISREG(stat.st_mode) || (uint32_t)stat.st_dev != image->device || stat.st_ino != image->inode ||
        stat.st_size < 0 || (uint64_t)stat.st_size != image->file_size || stat.st_gen != image->generation ||
        stat.st_mtimespec.tv_sec != image->modification_seconds || stat.st_mtimespec.tv_nsec != image->modification_nanoseconds ||
        stat.st_ctimespec.tv_sec != image->change_seconds || stat.st_ctimespec.tv_nsec != image->change_nanoseconds ||
        stat.st_birthtimespec.tv_sec != image->birth_seconds || stat.st_birthtimespec.tv_nsec != image->birth_nanoseconds)
        return failure(error, "image_changed", "runtime descriptor is not the observed mapped backing object");
    return 1;
}

int dp_runtime_revalidate(const dp_runtime_image *image, int fd, dp_runtime_error *error) {
    if (!image || image->abi_version != DP_RUNTIME_ABI || image->kind >= DP_RUNTIME_ROLES ||
        image->header_file_offset > image->file_size)
        return failure(error, "image_invalid", "invalid live runtime image observation");
    dp_runtime_image current;
    if (!dp_runtime_capture(image->kind, &current, error) || !descriptor_matches(image, fd, error)) return 0;
    if (!same_image(image, &current)) return failure(error, "mapping_changed", "current mapped runtime identity differs");
    struct mach_header_64 header;
    if (!file_read(fd, image->header_file_offset, &header, sizeof(header), error) || !header_valid(&header, image->kind, error)) return 0;
    if ((uint32_t)header.cputype != image->cpu_type || (uint32_t)header.cpusubtype != image->cpu_subtype || header.filetype != image->file_type ||
        sizeof(header) + (uint64_t)header.sizeofcmds > image->file_size - image->header_file_offset)
        return failure(error, "image_changed", "descriptor Mach-O header differs from loaded image");
    unsigned char *commands = malloc(header.sizeofcmds);
    if (!commands) return failure(error, "image_capacity", "allocate bounded descriptor header observation");
    uint8_t uuid[16] = {0};
    int valid = file_read(fd, image->header_file_offset + sizeof(header), commands, header.sizeofcmds, error) &&
        command_uuid(commands, &header, uuid, error);
    free(commands);
    if (!valid) return 0;
    if (memcmp(uuid, image->uuid, 16)) return failure(error, "image_changed", "descriptor UUID differs from loaded Mach-O UUID");
    if (!descriptor_matches(image, fd, error) || !dp_runtime_capture(image->kind, &current, error)) return 0;
    return same_image(image, &current) ? 1 : failure(error, "mapping_changed", "runtime changed during descriptor validation");
}

static int text_sysctl(const char *key, char *output, size_t capacity, dp_runtime_error *error) {
    size_t size = capacity;
    if (sysctlbyname(key, output, &size, NULL, 0) || size < 2 || size > capacity ||
        output[size - 1] || memchr(output, 0, size - 1))
        return failure(error, "platform_unavailable", "bounded runtime platform observation failed");
    return 1;
}

int dp_runtime_observe_platform(dp_runtime_platform *platform, dp_runtime_error *error) {
    if (!platform) return failure(error, "image_invalid", "missing runtime platform output");
    memset(platform, 0, sizeof(*platform));
    size_t size = sizeof(platform->cpu_family);
    if (!text_sysctl("kern.osversion", platform->os_build, sizeof(platform->os_build), error) ||
        !text_sysctl("hw.model", platform->hardware_model, sizeof(platform->hardware_model), error)) return 0;
    if (sysctlbyname("hw.cpufamily", &platform->cpu_family, &size, NULL, 0) || size != sizeof(platform->cpu_family) || !platform->cpu_family)
        return failure(error, "platform_unavailable", "bounded CPU family observation failed");
    platform->abi_version = DP_RUNTIME_ABI;
    return 1;
}

#else
int dp_runtime_capture(uint32_t kind, dp_runtime_image *image, dp_runtime_error *error) {
    (void)kind; (void)image;
    return failure(error, "unsupported", "mapped runtime identity requires macOS");
}
int dp_runtime_revalidate(const dp_runtime_image *image, int fd, dp_runtime_error *error) {
    (void)image; (void)fd;
    return failure(error, "unsupported", "mapped runtime identity requires macOS");
}
int dp_runtime_observe_platform(dp_runtime_platform *platform, dp_runtime_error *error) {
    (void)platform;
    return failure(error, "unsupported", "runtime platform identity requires macOS");
}
#endif
