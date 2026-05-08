/**
 * topk.cu — Top-K selection kernel (min-heap per row)
 */
#include <cstdint>
#include <cuda_runtime.h>
#include <cfloat>

struct Pair { float value; unsigned int index; };

__device__ void insert_sorted(Pair* heap, int k, Pair candidate) {
    if (candidate.value <= heap[0].value) return;
    heap[0] = candidate;
    int pos = 0;
    while (true) {
        int left = 2 * pos + 1, right = 2 * pos + 2, smallest = pos;
        if (left < k && heap[left].value < heap[smallest].value) smallest = left;
        if (right < k && heap[right].value < heap[smallest].value) smallest = right;
        if (smallest == pos) break;
        Pair tmp = heap[pos]; heap[pos] = heap[smallest]; heap[smallest] = tmp;
        pos = smallest;
    }
}

__global__ void topk_kernel(
    const float* similarities, float* values_out, unsigned int* indices_out,
    uint32_t batch_size, uint32_t elements_per_batch, uint32_t k
) {
    int batch_idx = blockIdx.x;
    if (batch_idx >= batch_size) return;

    const float* row = similarities + batch_idx * elements_per_batch;
    float* val_out = values_out + batch_idx * k;
    unsigned int* idx_out = indices_out + batch_idx * k;

    extern __shared__ char smem[];
    Pair* heap = reinterpret_cast<Pair*>(smem);

    if (threadIdx.x == 0) {
        for (int i = 0; i < (int)k; i++) {
            heap[i].value = (i < (int)elements_per_batch) ? row[i] : -FLT_MAX;
            heap[i].index = i;
        }
        for (int i = k / 2 - 1; i >= 0; i--) {
            int pos = i;
            while (true) {
                int left = 2*pos+1, right = 2*pos+2, smallest = pos;
                if (left < (int)k && heap[left].value < heap[smallest].value) smallest = left;
                if (right < (int)k && heap[right].value < heap[smallest].value) smallest = right;
                if (smallest == pos) break;
                Pair tmp = heap[pos]; heap[pos] = heap[smallest]; heap[smallest] = tmp;
                pos = smallest;
            }
        }
        for (int i = k; i < (int)elements_per_batch; i++) {
            insert_sorted(heap, (int)k, {row[i], (unsigned int)i});
        }
        for (int i = 0; i < (int)k; i++) {
            val_out[i] = heap[i].value;
            idx_out[i] = heap[i].index;
        }
    }
}

extern "C" int launch_cutlass_topk(
    uint64_t similarities_dev, uint64_t values_out_dev, uint64_t indices_out_dev,
    uint32_t batch_size, uint32_t elements_per_batch, uint32_t k,
    cudaStream_t stream
) {
    const float* similarities = reinterpret_cast<const float*>(similarities_dev);
    float* values_out = reinterpret_cast<float*>(values_out_dev);
    unsigned int* indices_out = reinterpret_cast<unsigned int*>(indices_out_dev);

    int block = 256;
    size_t shared_mem = k * sizeof(Pair);
    topk_kernel<<<(int)batch_size, block, shared_mem, stream>>>(
        similarities, values_out, indices_out, batch_size, elements_per_batch, k);
    return (cudaGetLastError() == cudaSuccess) ? 0 : -1;
}