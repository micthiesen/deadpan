// Bounded generated-fixture AVFoundation reader qualification.
// No playback, authored timing repair, or manual trim application.
#import <AVFoundation/AVFoundation.h>
#import <AudioToolbox/AudioToolbox.h>
#import <CommonCrypto/CommonDigest.h>
#import <CoreMedia/CoreMedia.h>
#import <Foundation/Foundation.h>
#import <dispatch/dispatch.h>

#include <errno.h>
#include <fcntl.h>
#include <math.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

enum {
    MaximumBuffers = 8192,
    MaximumTimingEntries = 8192,
    MaximumInspectedAttachments = 262144,
    MaximumSelectedAttachments = 8192,
    MaximumPCMFrames = 192000,
    MaximumInputBytes = 64 * 1024 * 1024,
    MaximumReportBytes = 16 * 1024 * 1024,
};

typedef struct {
    size_t timingEntries;
    size_t inspectedAttachments;
    size_t selectedAttachments;
} ObservationBudget;

static void requireCondition(BOOL condition, NSString *message) {
    if (!condition) {
        @throw [NSException exceptionWithName:@"ProbeFailure" reason:message userInfo:nil];
    }
}

static NSString *boundedString(NSString *value) {
    if (value == nil) return @"unspecified error";
    return value.length <= 2048 ? value : [value substringToIndex:2048];
}

static NSString *systemError(NSString *operation) {
    int code = errno;
    return [NSString stringWithFormat:@"%@: errno %d (%s)", operation, code, strerror(code)];
}

static NSDictionary *timeJSON(CMTime time) {
    return @{@"value": @(time.value), @"timescale": @(time.timescale),
             @"flags": @(time.flags), @"epoch": @(time.epoch)};
}

static NSDictionary *rangeJSON(CMTimeRange range) {
    return @{@"start": timeJSON(range.start), @"duration": timeJSON(range.duration)};
}

static NSDictionary *asbdJSON(const AudioStreamBasicDescription *asbd) {
    requireCondition(asbd != NULL && isfinite(asbd->mSampleRate), @"missing or nonfinite audio format");
    return @{@"sample_rate": @(asbd->mSampleRate), @"format_id": @(asbd->mFormatID),
             @"format_flags": @(asbd->mFormatFlags), @"bytes_per_packet": @(asbd->mBytesPerPacket),
             @"frames_per_packet": @(asbd->mFramesPerPacket), @"bytes_per_frame": @(asbd->mBytesPerFrame),
             @"channels_per_frame": @(asbd->mChannelsPerFrame), @"bits_per_channel": @(asbd->mBitsPerChannel)};
}

static void validateSourceASBD(const AudioStreamBasicDescription *asbd) {
    requireCondition(asbd != NULL && asbd->mFormatID == kAudioFormatMPEG4AAC &&
                     asbd->mSampleRate == 48000.0 && asbd->mChannelsPerFrame == 2,
                     @"fixture source must be 48 kHz stereo AAC");
}

static void validatePCM(const AudioStreamBasicDescription *asbd) {
    requireCondition(asbd != NULL && asbd->mFormatID == kAudioFormatLinearPCM &&
                     asbd->mSampleRate == 48000.0 && asbd->mChannelsPerFrame == 2 &&
                     asbd->mBitsPerChannel == 32 && asbd->mBytesPerFrame == 8 &&
                     asbd->mFramesPerPacket == 1 && asbd->mBytesPerPacket == 8 &&
                     asbd->mFormatFlags == (kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked),
                     @"returned PCM is not packed interleaved f32le stereo at 48 kHz");
}

static BOOL sameFile(const struct stat *a, const struct stat *b) {
    return a->st_dev == b->st_dev && a->st_ino == b->st_ino && a->st_mode == b->st_mode &&
           a->st_nlink == b->st_nlink && a->st_size == b->st_size &&
           a->st_mtimespec.tv_sec == b->st_mtimespec.tv_sec &&
           a->st_mtimespec.tv_nsec == b->st_mtimespec.tv_nsec &&
           a->st_ctimespec.tv_sec == b->st_ctimespec.tv_sec &&
           a->st_ctimespec.tv_nsec == b->st_ctimespec.tv_nsec;
}

static NSString *hashInput(int descriptor, off_t size) {
    CC_SHA256_CTX context;
    requireCondition(CC_SHA256_Init(&context) == 1, @"initialize input SHA-256");
    unsigned char bytes[65536];
    off_t offset = 0;
    while (offset < size) {
        size_t wanted = (size_t)MIN((off_t)sizeof(bytes), size - offset);
        ssize_t count = pread(descriptor, bytes, wanted, offset);
        if (count < 0 && errno == EINTR) continue;
        requireCondition(count > 0, count < 0 ? systemError(@"read input hash") : @"input shortened during hashing");
        requireCondition(CC_SHA256_Update(&context, bytes, (CC_LONG)count) == 1, @"update input SHA-256");
        offset += count;
    }
    unsigned char digest[CC_SHA256_DIGEST_LENGTH];
    requireCondition(CC_SHA256_Final(digest, &context) == 1, @"finish input SHA-256");
    char text[CC_SHA256_DIGEST_LENGTH * 2 + 1];
    for (size_t i = 0; i < sizeof(digest); i++) snprintf(text + i * 2, 3, "%02x", digest[i]);
    return [NSString stringWithUTF8String:text];
}

static void writeAll(int descriptor, const void *bytes, size_t size) {
    const unsigned char *cursor = bytes;
    while (size > 0) {
        ssize_t count = write(descriptor, cursor, size);
        if (count < 0 && errno == EINTR) continue;
        requireCondition(count > 0, count < 0 ? systemError(@"write output") : @"zero-length output write");
        cursor += count;
        size -= (size_t)count;
    }
}

static void loadKeys(id<AVAsynchronousKeyValueLoading> object, NSArray<NSString *> *keys) {
    dispatch_semaphore_t ready = dispatch_semaphore_create(0);
    [object loadValuesAsynchronouslyForKeys:keys completionHandler:^{ dispatch_semaphore_signal(ready); }];
    requireCondition(dispatch_semaphore_wait(ready, dispatch_time(DISPATCH_TIME_NOW, 30 * NSEC_PER_SEC)) == 0,
                     @"timed out loading asset metadata");
    for (NSString *key in keys) {
        NSError *error = nil;
        AVKeyValueStatus status = [object statusOfValueForKey:key error:&error];
        requireCondition(status == AVKeyValueStatusLoaded,
                         [NSString stringWithFormat:@"load %@: %@", key, boundedString(error.localizedDescription)]);
    }
}

static AVAssetTrack *audioTrack(AVURLAsset *asset) {
    dispatch_semaphore_t ready = dispatch_semaphore_create(0);
    __block NSArray<AVAssetTrack *> *tracks = nil;
    __block NSError *error = nil;
    [asset loadTracksWithMediaType:AVMediaTypeAudio completionHandler:^(NSArray<AVAssetTrack *> *value, NSError *failure) {
        tracks = value;
        error = failure;
        dispatch_semaphore_signal(ready);
    }];
    requireCondition(dispatch_semaphore_wait(ready, dispatch_time(DISPATCH_TIME_NOW, 30 * NSEC_PER_SEC)) == 0,
                     @"timed out loading audio tracks");
    requireCondition(error == nil && tracks != nil,
                     [NSString stringWithFormat:@"load audio tracks: %@", boundedString(error.localizedDescription)]);
    requireCondition(tracks.count == 1, @"fixture must contain exactly one audio track");
    return tracks[0];
}

static id timeAttachment(CMSampleBufferRef sample, CFStringRef key) {
    CFTypeRef value = CMGetAttachment(sample, key, NULL);
    if (value == NULL) return NSNull.null;
    requireCondition(CFGetTypeID(value) == CFDictionaryGetTypeID(), @"trim attachment is not a CMTime dictionary");
    return timeJSON(CMTimeMakeFromDictionary((CFDictionaryRef)value));
}

static id boolAttachment(CMSampleBufferRef sample, CFStringRef key) {
    CFTypeRef value = CMGetAttachment(sample, key, NULL);
    if (value == NULL) return NSNull.null;
    requireCondition(CFGetTypeID(value) == CFBooleanGetTypeID(), @"boolean attachment has unexpected type");
    return @(CFBooleanGetValue((CFBooleanRef)value));
}

static id speedAttachment(CMSampleBufferRef sample) {
    CFTypeRef value = CMGetAttachment(sample, kCMSampleBufferAttachmentKey_SpeedMultiplier, NULL);
    if (value == NULL) return NSNull.null;
    requireCondition(CFGetTypeID(value) == CFNumberGetTypeID(), @"speed attachment is not numeric");
    double speed = 0;
    requireCondition(CFNumberGetValue((CFNumberRef)value, kCFNumberDoubleType, &speed) && isfinite(speed),
                     @"speed attachment is not a finite number");
    return @(speed);
}

static NSDictionary *attachmentsJSON(CMSampleBufferRef sample) {
    return @{@"trim_start": timeAttachment(sample, kCMSampleBufferAttachmentKey_TrimDurationAtStart),
             @"trim_end": timeAttachment(sample, kCMSampleBufferAttachmentKey_TrimDurationAtEnd),
             @"speed": speedAttachment(sample),
             @"reverse": boolAttachment(sample, kCMSampleBufferAttachmentKey_Reverse),
             @"empty_media": boolAttachment(sample, kCMSampleBufferAttachmentKey_EmptyMedia),
             @"reset_decoder": boolAttachment(sample, kCMSampleBufferAttachmentKey_ResetDecoderBeforeDecoding),
             @"drain_after_decoding": boolAttachment(sample, kCMSampleBufferAttachmentKey_DrainAfterDecoding)};
}

static NSArray *timingJSON(CMSampleBufferRef sample, CMItemCount samples, ObservationBudget *budget) {
    CMItemCount count = 0;
    OSStatus status = CMSampleBufferGetSampleTimingInfoArray(sample, 0, NULL, &count);
    if (samples == 0 && status == kCMSampleBufferError_BufferHasNoSampleTimingInfo) return @[];
    requireCondition(status == noErr && count >= 0 && (uint64_t)count <= MaximumTimingEntries - budget->timingEntries,
                     @"invalid or excessive sample timing entries");
    if (count == 0) {
        requireCondition(samples == 0, @"nonempty sample buffer has no timing entries");
        return @[];
    }
    CMSampleTimingInfo *entries = calloc((size_t)count, sizeof(*entries));
    requireCondition(entries != NULL, @"allocate bounded sample timing entries");
    NSMutableArray *result = [NSMutableArray arrayWithCapacity:(NSUInteger)count];
    @try {
        CMItemCount returned = 0;
        status = CMSampleBufferGetSampleTimingInfoArray(sample, count, entries, &returned);
        requireCondition(status == noErr && returned == count, @"sample timing array changed or failed");
        budget->timingEntries += (size_t)count;
        for (CMItemCount i = 0; i < count; i++) {
            [result addObject:@{@"pts": timeJSON(entries[i].presentationTimeStamp),
                               @"dts": timeJSON(entries[i].decodeTimeStamp),
                               @"duration": timeJSON(entries[i].duration)}];
        }
    } @finally {
        free(entries);
    }
    return result;
}

static void sampleAttachmentsJSON(CMSampleBufferRef sample, NSMutableDictionary *record, ObservationBudget *budget) {
    CFArrayRef entries = CMSampleBufferGetSampleAttachmentsArray(sample, false);
    CFIndex count = entries == NULL ? 0 : CFArrayGetCount(entries);
    requireCondition(count >= 0 && count <= CMSampleBufferGetNumSamples(sample) &&
                     (uint64_t)count <= MaximumInspectedAttachments - budget->inspectedAttachments,
                     @"excessive per-sample attachment inventory");
    budget->inspectedAttachments += (size_t)count;
    record[@"sample_attachment_count"] = @(count);
    NSMutableArray *selected = [NSMutableArray array];
    for (CFIndex i = 0; i < count; i++) {
        CFTypeRef item = CFArrayGetValueAtIndex(entries, i);
        requireCondition(item != NULL && CFGetTypeID(item) == CFDictionaryGetTypeID(), @"invalid sample attachment dictionary");
        CFTypeRef value = CFDictionaryGetValue((CFDictionaryRef)item, kCMSampleAttachmentKey_DoNotDisplay);
        if (value != NULL) {
            requireCondition(CFGetTypeID(value) == CFBooleanGetTypeID(), @"DoNotDisplay attachment is not boolean");
            requireCondition(budget->selectedAttachments < MaximumSelectedAttachments, @"excessive selected sample attachments");
            budget->selectedAttachments++;
            [selected addObject:@{@"sample_index": @(i), @"do_not_display": @(CFBooleanGetValue((CFBooleanRef)value))}];
        }
    }
    record[@"sample_attachments"] = selected;
}

static NSMutableDictionary *bufferJSON(CMSampleBufferRef sample, NSUInteger index, ObservationBudget *budget) {
    CMItemCount samples = CMSampleBufferGetNumSamples(sample);
    requireCondition(samples >= 0 && samples <= MaximumPCMFrames, @"invalid or excessive sample-buffer sample count");
    CMFormatDescriptionRef description = CMSampleBufferGetFormatDescription(sample);
    const AudioStreamBasicDescription *asbd = description == NULL ? NULL :
        CMAudioFormatDescriptionGetStreamBasicDescription(description);
    NSMutableDictionary *record = [@{@"index": @(index), @"num_samples": @(samples),
        @"data_ready": @(CMSampleBufferDataIsReady(sample)),
        @"pts": timeJSON(CMSampleBufferGetPresentationTimeStamp(sample)),
        @"dts": timeJSON(CMSampleBufferGetDecodeTimeStamp(sample)),
        @"duration": timeJSON(CMSampleBufferGetDuration(sample)),
        @"output_pts": timeJSON(CMSampleBufferGetOutputPresentationTimeStamp(sample)),
        @"output_duration": timeJSON(CMSampleBufferGetOutputDuration(sample)),
        @"timing": timingJSON(sample, samples, budget),
        @"asbd": asbd == NULL ? (id)NSNull.null : asbdJSON(asbd),
        @"attachments": attachmentsJSON(sample)} mutableCopy];
    sampleAttachmentsJSON(sample, record, budget);
    return record;
}

static void appendPCM(CMSampleBufferRef sample, int output, NSMutableDictionary *record, uint64_t *total) {
    CMItemCount samples = CMSampleBufferGetNumSamples(sample);
    requireCondition(samples >= 0 && (uint64_t)samples <= MaximumPCMFrames - *total, @"decoded PCM frame cap exceeded");
    record[@"sample_offset"] = @(*total);
    record[@"byte_count"] = @0;
    if (samples == 0) return;
    requireCondition(CMSampleBufferDataIsReady(sample), @"decoded PCM data is not ready");
    CMFormatDescriptionRef description = CMSampleBufferGetFormatDescription(sample);
    requireCondition(description != NULL, @"decoded PCM has no format description");
    validatePCM(CMAudioFormatDescriptionGetStreamBasicDescription(description));

    AudioBufferList list = {0};
    CMBlockBufferRef backing = NULL;
    @try {
        size_t needed = 0;
        OSStatus status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(sample, &needed,
            &list, sizeof(list), kCFAllocatorDefault, kCFAllocatorDefault,
            kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment, &backing);
        requireCondition(status == noErr && needed <= sizeof(list) && backing != NULL,
                         @"obtain bounded interleaved PCM buffer");
        size_t bytes = (size_t)samples * 8;
        requireCondition(list.mNumberBuffers == 1 && list.mBuffers[0].mNumberChannels == 2 &&
                         list.mBuffers[0].mData != NULL && list.mBuffers[0].mDataByteSize == bytes,
                         @"PCM sample count and returned buffer bytes disagree");
        const float *pcm = list.mBuffers[0].mData;
        for (size_t i = 0; i < (size_t)samples * 2; i++) {
            requireCondition(isfinite(pcm[i]), @"decoded PCM contains nonfinite values");
        }
        writeAll(output, list.mBuffers[0].mData, bytes);
        record[@"byte_count"] = @(bytes);
        *total += (uint64_t)samples;
    } @finally {
        if (backing != NULL) CFRelease(backing);
    }
}

static void readPass(AVURLAsset *asset, AVAssetTrack *track, BOOL pcm, int output, NSMutableDictionary *root) {
    NSString *key = pcm ? @"pcm" : @"stored";
    NSMutableArray *buffers = [NSMutableArray array];
    NSMutableDictionary *pass = [@{@"status": @"reading", @"reader_status": @(AVAssetReaderStatusUnknown),
                                  @"buffer_count": @0, @"buffers": buffers} mutableCopy];
    root[key] = pass;
    uint64_t total = 0;
    if (pcm) {
        pass[@"total_samples"] = @0;
        pass[@"pcm_bytes"] = @0;
        pass[@"pcm_format"] = @"f32le_interleaved_stereo_48000";
    }
    NSError *error = nil;
    AVAssetReader *reader = [[AVAssetReader alloc] initWithAsset:asset error:&error];
    requireCondition(reader != nil, [NSString stringWithFormat:@"create reader: %@", boundedString(error.localizedDescription)]);
    @try {
        NSDictionary *settings = pcm ? @{AVFormatIDKey: @(kAudioFormatLinearPCM),
            AVSampleRateKey: @48000, AVNumberOfChannelsKey: @2, AVLinearPCMBitDepthKey: @32,
            AVLinearPCMIsFloatKey: @YES, AVLinearPCMIsBigEndianKey: @NO,
            AVLinearPCMIsNonInterleaved: @NO} : nil;
        AVAssetReaderTrackOutput *trackOutput = [[AVAssetReaderTrackOutput alloc] initWithTrack:track outputSettings:settings];
        requireCondition(trackOutput != nil, @"create track output");
        trackOutput.alwaysCopiesSampleData = NO;
        // Default timeRange and supportsRandomAccess are intentional. Never trim or reset.
        requireCondition([reader canAddOutput:trackOutput], @"reader cannot add requested track output");
        [reader addOutput:trackOutput];
        requireCondition([reader startReading], [NSString stringWithFormat:@"start reader: %@", boundedString(reader.error.localizedDescription)]);
        ObservationBudget budget = {0};
        for (;;) {
            @autoreleasepool {
                CMSampleBufferRef sample = [trackOutput copyNextSampleBuffer];
                if (sample == NULL) break;
                @try {
                    requireCondition(buffers.count < MaximumBuffers, @"sample buffer count cap exceeded");
                    NSMutableDictionary *record = bufferJSON(sample, buffers.count, &budget);
                    [buffers addObject:record];
                    pass[@"buffer_count"] = @(buffers.count);
                    if (pcm) {
                        appendPCM(sample, output, record, &total);
                        pass[@"total_samples"] = @(total);
                        pass[@"pcm_bytes"] = @(total * 8);
                    } else {
                        requireCondition(CMSampleBufferGetNumSamples(sample) == 0 || CMSampleBufferDataIsReady(sample),
                                         @"stored-sample data is not ready");
                    }
                } @finally {
                    CFRelease(sample);
                }
            }
        }
        pass[@"reader_status"] = @(reader.status);
        requireCondition(reader.status == AVAssetReaderStatusCompleted,
                         [NSString stringWithFormat:@"reader ended with status %ld: %@", (long)reader.status,
                          boundedString(reader.error.localizedDescription)]);
        requireCondition(buffers.count > 0 && (!pcm || total > 0), @"reader completed without fixture media");
        pass[@"status"] = @"completed";
    } @catch (NSException *failure) {
        pass[@"status"] = @"failed";
        pass[@"reader_status"] = @(reader.status);
        pass[@"error"] = boundedString(failure.reason);
        @throw;
    } @finally {
        // All copyNextSampleBuffer calls have returned before cancellation.
        if (reader.status == AVAssetReaderStatusReading) [reader cancelReading];
    }
}

static AVAssetTrack *describeAsset(AVURLAsset *asset, NSMutableDictionary *root) {
    loadKeys(asset, @[@"duration", @"providesPreciseDurationAndTiming"]);
    CMTime duration = asset.duration;
    root[@"asset_duration"] = timeJSON(duration);
    root[@"provides_precise_duration_and_timing"] = @(asset.providesPreciseDurationAndTiming);
    requireCondition(asset.providesPreciseDurationAndTiming && CMTIME_IS_NUMERIC(duration) &&
                     !CMTIME_HAS_BEEN_ROUNDED(duration) && duration.epoch == 0 &&
                     CMTimeCompare(duration, kCMTimeZero) > 0 && CMTimeCompare(duration, CMTimeMake(4, 1)) <= 0,
                     @"fixture asset must have a precise positive duration no greater than four seconds");
    AVAssetTrack *track = audioTrack(asset);
    loadKeys(track, @[@"formatDescriptions", @"timeRange", @"segments"]);
    NSArray *descriptions = track.formatDescriptions;
    requireCondition(descriptions.count > 0 && descriptions.count <= 8, @"bounded source audio format inventory");
    NSMutableArray *formats = [NSMutableArray array];
    for (id item in descriptions) {
        CMFormatDescriptionRef description = (__bridge CMFormatDescriptionRef)item;
        requireCondition(CFGetTypeID(description) == CMFormatDescriptionGetTypeID() &&
                         CMFormatDescriptionGetMediaType(description) == kCMMediaType_Audio,
                         @"invalid source audio format description");
        const AudioStreamBasicDescription *asbd = CMAudioFormatDescriptionGetStreamBasicDescription(description);
        [formats addObject:asbdJSON(asbd)];
        validateSourceASBD(asbd);
    }
    NSArray<AVAssetTrackSegment *> *segments = track.segments;
    requireCondition(segments.count <= 64, @"bounded track segment inventory");
    NSMutableArray *segmentRecords = [NSMutableArray array];
    for (AVAssetTrackSegment *segment in segments) {
        CMTimeMapping mapping = segment.timeMapping;
        [segmentRecords addObject:@{@"empty": @(segment.empty),
                                   @"source": rangeJSON(mapping.source), @"target": rangeJSON(mapping.target)}];
    }
    root[@"track"] = @{@"id": @(track.trackID), @"time_range": rangeJSON(track.timeRange),
                       @"source_formats": formats, @"segments": segmentRecords};
    return track;
}

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        if (argc != 3) {
            fputs("usage: avfoundation_probe INPUT_MP4 OUTPUT_PCM_F32LE\n", stderr);
            return 2;
        }
        NSMutableDictionary *report = [@{@"schema_version": @1, @"kind": @"avfoundation_audio", @"status": @"failed"} mutableCopy];
        int input = -1, output = -1;
        int result = 1;
        @try {
            NSString *inputPath = [NSString stringWithUTF8String:argv[1]];
            NSString *outputPath = [NSString stringWithUTF8String:argv[2]];
            requireCondition(inputPath != nil && outputPath != nil && inputPath.length <= 4096 && outputPath.length <= 4096,
                             @"paths must be bounded UTF-8 strings");
            report[@"input_path"] = inputPath;
            report[@"pcm_path"] = outputPath;
            input = open(argv[1], O_RDONLY | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC);
            requireCondition(input >= 0, systemError(@"open input"));
            struct stat initial;
            requireCondition(fstat(input, &initial) == 0, systemError(@"inspect input"));
            requireCondition(S_ISREG(initial.st_mode) && initial.st_size > 0 && initial.st_size <= MaximumInputBytes,
                             @"input must be a nonempty regular fixture no larger than 64 MiB");
            NSString *initialHash = hashInput(input, initial.st_size);
            report[@"input_sha256"] = initialHash;
            struct stat checked;
            requireCondition(fstat(input, &checked) == 0 && sameFile(&initial, &checked) &&
                             lstat(argv[1], &checked) == 0 && sameFile(&initial, &checked), @"input changed before reader admission");
            output = open(argv[2], O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0600);
            requireCondition(output >= 0, systemError(@"create PCM exclusively"));

            AVURLAsset *asset = [[AVURLAsset alloc] initWithURL:[NSURL fileURLWithPath:inputPath]
                options:@{AVURLAssetPreferPreciseDurationAndTimingKey: @YES}];
            AVAssetTrack *track = describeAsset(asset, report);
            readPass(asset, track, NO, output, report);
            readPass(asset, track, YES, output, report);

            NSString *finalHash = hashInput(input, initial.st_size);
            report[@"final_input_sha256"] = finalHash;
            requireCondition([initialHash isEqualToString:finalHash] && fstat(input, &checked) == 0 &&
                             sameFile(&initial, &checked) && lstat(argv[1], &checked) == 0 && sameFile(&initial, &checked),
                             @"input bytes or identity changed while reading");
            requireCondition(fsync(output) == 0, systemError(@"synchronize PCM"));
            int closing = output;
            output = -1;
            requireCondition(close(closing) == 0, systemError(@"close PCM"));
            closing = input;
            input = -1;
            requireCondition(close(closing) == 0, systemError(@"close input"));
            report[@"status"] = @"completed";
            result = 0;
        } @catch (NSException *failure) {
            report[@"status"] = @"failed";
            report[@"error"] = boundedString(failure.reason);
            fprintf(stderr, "%s\n", [boundedString(failure.reason) UTF8String]);
        } @finally {
            if (output >= 0) close(output);
            if (input >= 0) close(input);
        }
        @try {
            NSError *error = nil;
            NSData *json = [NSJSONSerialization dataWithJSONObject:report options:NSJSONWritingSortedKeys error:&error];
            requireCondition(json != nil && json.length <= MaximumReportBytes,
                             [NSString stringWithFormat:@"serialize bounded report: %@", boundedString(error.localizedDescription)]);
            writeAll(STDOUT_FILENO, json.bytes, json.length);
            writeAll(STDOUT_FILENO, "\n", 1);
        } @catch (NSException *failure) {
            fprintf(stderr, "%s\n", [boundedString(failure.reason) UTF8String]);
            return 1;
        }
        return result;
    }
}
