// cxx-bridge/dali_wrapper.h — Thin C++ wrapper around DALI Pipeline
#pragma once
#include "rust/cxx.h"
#include <memory>
#include <string>
#include <vector>
#include <cstdint>

namespace dali { class Pipeline; }

class DaliPipelineHandle {
public:
    DaliPipelineHandle(int max_batch_size, int num_threads, int device_id);
    ~DaliPipelineHandle();

    bool add_operator(rust::Str serialized_spec, rust::Str inst_name) const;
    bool build() const;
    bool run() const;

    size_t output_count() const;
    rust::Vec<int64_t> output_shape(size_t idx) const;
    uint32_t output_dtype(size_t idx) const;
    int output_device(size_t idx) const;

    size_t copy_output_to_host(size_t idx, rust::Slice<float> dst) const;

private:
    std::unique_ptr<dali::Pipeline> pipeline_;
    mutable bool built_ = false;
};

std::unique_ptr<DaliPipelineHandle> dali_create_pipeline(
    int max_batch_size,
    int num_threads,
    int device_id
);