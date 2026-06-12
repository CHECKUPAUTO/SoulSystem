// cxx-bridge/dali_wrapper.cpp — Implementation of the thin DALI C++ wrapper
#include "dali_wrapper.h"
#include "dali/pipeline/pipeline.h"

#include <cstring>
#include <cuda_runtime.h>

// ── Constructor / Destructor ────────────────────────────────────────────────

DaliPipelineHandle::DaliPipelineHandle(int max_batch_size, int num_threads, int device_id)
    : pipeline_(std::make_unique<dali::Pipeline>(max_batch_size, num_threads, device_id,
                                                  0, true, 2, true, false)) {}

DaliPipelineHandle::~DaliPipelineHandle() = default;

// ── Operators ──────────────────────────────────────────────────────────────

bool DaliPipelineHandle::add_operator(rust::Str serialized_spec, rust::Str inst_name) const {
    try {
        dali::OpSpec spec(std::string(serialized_spec.data(), serialized_spec.size()));
        pipeline_->AddOperator(spec, std::string(inst_name.data(), inst_name.size()));
        return true;
    } catch (...) {
        return false;
    }
}

// ── Build & Run ────────────────────────────────────────────────────────────

bool DaliPipelineHandle::build() const {
    try {
        pipeline_->Build();
        built_ = true;
        return true;
    } catch (...) {
        return false;
    }
}

bool DaliPipelineHandle::run() const {
    if (!built_) return false;
    try {
        pipeline_->Run();
        return true;
    } catch (...) {
        return false;
    }
}

// ── Output query ────────────────────────────────────────────────────────────

size_t DaliPipelineHandle::output_count() const {
    return pipeline_->output_descs().size();
}

rust::Vec<int64_t> DaliPipelineHandle::output_shape(size_t idx) const {
    rust::Vec<int64_t> shape;
    (void)idx;
    return shape;
}

uint32_t DaliPipelineHandle::output_dtype(size_t idx) const {
    return static_cast<uint32_t>(pipeline_->output_dtype(idx));
}

int DaliPipelineHandle::output_device(size_t idx) const {
    auto dev = pipeline_->output_device(idx);
    return (dev == dali::StorageDevice::GPU) ? 1 : 0;
}

size_t DaliPipelineHandle::copy_output_to_host(size_t idx, rust::Slice<float> dst) const {
    (void)idx;
    (void)dst;
    return 0;
}

// ── Factory ────────────────────────────────────────────────────────────────

std::unique_ptr<DaliPipelineHandle> dali_create_pipeline(
    int max_batch_size,
    int num_threads,
    int device_id) {
    return std::make_unique<DaliPipelineHandle>(max_batch_size, num_threads, device_id);
}