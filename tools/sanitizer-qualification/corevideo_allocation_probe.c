/* Where CoreVideo puts the BGRA pixel buffers deadpan-track writes into.
 *
 * deadpan-track copies decoded pictures into CVPixelBufferCreate buffers
 * through a raw slice (src/vision.rs, pixel_buffer). This probe shows whether
 * those buffers come from the AddressSanitizer allocator, so that an
 * out-of-bounds write by the instrumented Rust copy would hit a redzone.
 *
 *   clang -fsanitize=address -g corevideo_allocation_probe.c \
 *     -framework CoreVideo -framework CoreFoundation -o /tmp/probe && /tmp/probe
 *
 * See docs/qualification/sanitizers-2026-10-15.md for the recorded output.
 */
#include <CoreVideo/CoreVideo.h>
#include <sanitizer/asan_interface.h>
#include <sanitizer/allocator_interface.h>
#include <stdio.h>
int main(void) {
  size_t sizes[][2] = {{64,36},{320,180},{1920,1080},{3840,2160}};
  for (int i = 0; i < 4; i++) {
    CVPixelBufferRef b = 0;
    CVReturn r = CVPixelBufferCreate(0, sizes[i][0], sizes[i][1], kCVPixelFormatType_32BGRA, 0, &b);
    CVPixelBufferLockBaseAddress(b, 0);
    unsigned char *base = CVPixelBufferGetBaseAddress(b);
    size_t stride = CVPixelBufferGetBytesPerRow(b), data = CVPixelBufferGetDataSize(b);
    int owned = __sanitizer_get_ownership(base);
    size_t alloc = owned ? __sanitizer_get_allocated_size(base) : 0;
    void *past = __asan_region_is_poisoned(base + data, 1);
    printf("%zux%zu r=%d stride=%zu stride*h=%zu data=%zu asan_owned=%d alloc=%zu past_end_poisoned=%d\n", sizes[i][0], sizes[i][1], r, stride, stride*sizes[i][1], data, owned, alloc, past != 0);
    CVPixelBufferUnlockBaseAddress(b, 0);
    CVPixelBufferRelease(b);
  }
}
