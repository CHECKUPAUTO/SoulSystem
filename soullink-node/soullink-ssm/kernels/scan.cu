// SoulLink SSM — CUDA parallel scan kernels
// Compile with nvcc -arch=sm_60 -O3 -use_fast_math

// ─── ScanElement (A, B pair for associative scan) ─────────────────────────
typedef struct {
    float a;  // Linear multiplier
    float b;  // Additive term
} ScanElement;

// ─── Block-level inclusive scan using shared memory ────────────────────────
// Each block processes BLOCK_SIZE elements using a tree-based reduction.
// Up-sweep: O(log N) parallel reduction
// Down-sweep: O(log N) parallel distribution

#define BLOCK_SIZE 256

__global__ void block_scan_kernel(const ScanElement* input, ScanElement* output, int n) {
    __shared__ ScanElement shared[BLOCK_SIZE * 2];

    int tid = threadIdx.x;
    int gid = blockIdx.x * blockDim.x + tid;

    // Load into shared memory
    if (gid < n) {
        shared[tid] = input[gid];
    } else {
        shared[tid].a = 1.0f;
        shared[tid].b = 0.0f;
    }
    __syncthreads();

    // Up-sweep (reduction)
    for (int stride = 1; stride < blockDim.x; stride *= 2) {
        int idx = (tid + 1) * stride * 2 - 1;
        if (idx < blockDim.x * 2) {
            int left = idx - stride;
            // Compose: (a,b) = (a2*a1, a2*b1 + b2)
            float a1 = shared[left].a, b1 = shared[left].b;
            float a2 = shared[idx].a, b2 = shared[idx].b;
            shared[idx].a = a2 * a1;
            shared[idx].b = a2 * b1 + b2;
        }
        __syncthreads();
    }

    // Down-sweep (distribution)
    for (int stride = blockDim.x / 2; stride > 0; stride /= 2) {
        int idx = (tid + 1) * stride * 2 - 1;
        if (idx + stride < blockDim.x * 2) {
            int right = idx + stride;
            float a1 = shared[idx].a, b1 = shared[idx].b;
            float a2 = shared[right].a, b2 = shared[right].b;
            shared[right].a = a2 * a1;
            shared[right].b = a2 * b1 + b2;
        }
        __syncthreads();
    }

    // Write output
    if (gid < n) {
        output[gid] = shared[tid];
    }
}

// ─── Full SSM scan (two-pass: block scan + inter-block propagation) ────────
// Phase 1: each block computes its scan and total reduction
// Phase 2: block totals are propagated across blocks (CPU-side)

__global__ void block_totals_kernel(const ScanElement* input, ScanElement* block_totals, int n) {
    __shared__ ScanElement shared[BLOCK_SIZE];

    int tid = threadIdx.x;
    int gid = blockIdx.x * blockDim.x + tid;

    // Load last element of each segment
    if (gid < n) {
        shared[tid] = input[gid];
    } else {
        shared[tid].a = 1.0f;
        shared[tid].b = 0.0f;
    }
    __syncthreads();

    // Tree reduction to compute block total
    for (int stride = 1; stride < blockDim.x; stride *= 2) {
        int idx = (tid + 1) * stride * 2 - 1;
        if (idx < blockDim.x) {
            int left = idx - stride;
            float a1 = shared[left].a, b1 = shared[left].b;
            float a2 = shared[idx].a, b2 = shared[idx].b;
            shared[idx].a = a2 * a1;
            shared[idx].b = a2 * b1 + b2;
        }
        __syncthreads();
    }

    if (tid == 0) {
        block_totals[blockIdx.x] = shared[blockDim.x - 1];
    }
}

// ─── Apply inter-block prefix to each block's output ──────────────────────
__global__ void apply_prefix_kernel(ScanElement* output, const ScanElement* prefixes, int n) {
    int gid = blockIdx.x * blockDim.x + threadIdx.x;
    if (gid >= n) return;

    int block_id = blockIdx.x;
    ScanElement prefix = prefixes[block_id];

    // output[gid] = output[gid] ∘ prefix
    float a1 = prefix.a, b1 = prefix.b;
    float a2 = output[gid].a, b2 = output[gid].b;
    output[gid].a = a2 * a1;
    output[gid].b = a2 * b1 + b2;
}

// ─── Wrapper: full SSM inclusive scan ─────────────────────────────────────
// Called from Rust via C FFI
extern "C" void run_inclusive_scan_cuda(
    const ScanElement* input,
    ScanElement* output,
    int n,
    int block_size
) {
    int num_blocks = (n + block_size - 1) / block_size;

    // Phase 1: block scan
    block_scan_kernel<<<num_blocks, block_size>>>(input, output, n);
    cudaDeviceSynchronize();

    // Phase 2: compute block totals
    ScanElement* d_block_totals;
    cudaMalloc(&d_block_totals, num_blocks * sizeof(ScanElement));
    block_totals_kernel<<<num_blocks, block_size>>>(output, d_block_totals, n);
    cudaDeviceSynchronize();

    // Copy block totals to CPU for prefix propagation
    ScanElement* h_block_totals = (ScanElement*)malloc(num_blocks * sizeof(ScanElement));
    cudaMemcpy(h_block_totals, d_block_totals, num_blocks * sizeof(ScanElement), cudaMemcpyDeviceToHost);

    // Compute exclusive prefixes on CPU
    ScanElement* h_prefixes = (ScanElement*)malloc(num_blocks * sizeof(ScanElement));
    h_prefixes[0].a = 1.0f;
    h_prefixes[0].b = 0.0f;
    for (int i = 1; i < num_blocks; i++) {
        float a1 = h_prefixes[i-1].a, b1 = h_prefixes[i-1].b;
        float a2 = h_block_totals[i-1].a, b2 = h_block_totals[i-1].b;
        h_prefixes[i].a = a2 * a1;
        h_prefixes[i].b = a2 * b1 + b2;
    }

    // Copy prefixes to device and apply
    ScanElement* d_prefixes;
    cudaMalloc(&d_prefixes, num_blocks * sizeof(ScanElement));
    cudaMemcpy(d_prefixes, h_prefixes, num_blocks * sizeof(ScanElement), cudaMemcpyHostToDevice);

    apply_prefix_kernel<<<num_blocks, block_size>>>(output, d_prefixes, n);
    cudaDeviceSynchronize();

    // Cleanup
    cudaFree(d_block_totals);
    cudaFree(d_prefixes);
    free(h_block_totals);
    free(h_prefixes);
}