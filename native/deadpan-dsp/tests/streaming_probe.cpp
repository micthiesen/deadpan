// Direct ABI validation plus a long-stream oracle using the original complete
// source interface. The reference never uses required_input or window staging.
#include "adapter.h"
#include "canonical.hpp"
#include <cassert>
#include <cstdio>
#include <limits>
#include <vector>

struct Source {
    std::array<std::vector<float>, 2> samples;
    const auto &operator[](int channel) const { return samples[channel]; }
};

int main() {
    constexpr std::uint32_t input_frames = 2 * 48000 * 60 + 17;
    Source source;
    for (auto &channel : source.samples) channel.resize(input_frames);
    for (std::uint32_t at = 0; at < input_frames; ++at) {
        source.samples[0][at] = float(int((std::uint64_t(at)*97)%16384)-8192)/32768;
        source.samples[1][at] = float(int((std::uint64_t(at)*43)%8192)-4096)/32768;
    }
    std::uint64_t compared = 0;
    std::uint32_t maximum_window = 0;
    for (auto rate : {deadpan_audio_probe::ExactRate{2,1}, {1000,1001}, {1,8}, {8,1}}) {
        // Slow-rate cases need only the initial 50k output samples to include
        // priming and many rolling windows; the 2x case consumes all two minutes.
        auto output_frames = rate.numerator == 2 ? input_frames/2 : 50003U;
        deadpan_audio_probe::CanonicalStretch<Source> reference(source,input_frames,output_frames,rate,7);
        void *engine = nullptr;
        assert(dp_dsp_stream_create(input_frames,output_frames,rate.numerator,rate.denominator,7,&engine)==0);
        std::uint32_t iteration = 0;
        for (;;) {
            std::uint32_t requested = (iteration++ % 3 == 0) ? 173 : 256;
            std::uint32_t start=0, frames=0, written=999;
            assert(dp_dsp_stream_required(engine,requested,&start,&frames)==0);
            maximum_window = std::max(maximum_window,frames);
            assert(frames<=65536 && std::uint64_t(start)+frames<=input_frames);
            std::array<float,256> left{},right{},expected_left{},expected_right{};
            left.fill(123); right.fill(456);
            // Invalid lengths must reject before dereferencing these null inputs.
            assert(dp_dsp_stream_read(engine,start,nullptr,nullptr,frames+1,left.data(),right.data(),requested,&written)==1);
            assert(written==0 && left[0]==123 && right[0]==456);
            assert(dp_dsp_stream_read(engine,start,source[0].data()+start,source[1].data()+start,frames,left.data(),right.data(),requested,&written)==0);
            auto expected = reference.read(expected_left.data(),expected_right.data(),int(requested));
            assert(written == std::uint32_t(expected));
            for (std::uint32_t at=0;at<written;++at) {
                assert(left[at]==expected_left[at] && right[at]==expected_right[at]);
                ++compared;
            }
            for (auto at=written;at<256;++at) assert(left[at]==123 && right[at]==456);
            if (!written) break;
        }
        dp_dsp_stream_destroy(engine);
    }
    void *engine = nullptr;
    assert(dp_dsp_stream_create(100,100,1,1,25,&engine)==1 && !engine);
    assert(dp_dsp_stream_create(100,100,1,1,0,&engine)==0);
    std::uint32_t start=0,frames=0,written=0;
    assert(dp_dsp_stream_required(engine,257,&start,&frames)==1);
    assert(dp_dsp_stream_required(engine,256,&start,&frames)==0);
    std::vector<float> input(frames,0);
    std::array<float,256> left{},right{};
    input[0]=std::numeric_limits<float>::quiet_NaN();
    assert(dp_dsp_stream_read(engine,start,input.data(),input.data(),frames,left.data(),right.data(),256,&written)==1);
    input[0]=0;
    assert(dp_dsp_stream_read(engine,start,input.data(),input.data(),frames,left.data(),right.data(),256,&written)==0);
    assert(written==100);
    dp_dsp_stream_destroy(engine);
    std::printf("{\"compared_stereo_frames\":%llu,\"maximum_input_window\":%u}\n",
                static_cast<unsigned long long>(compared),maximum_window);
}
