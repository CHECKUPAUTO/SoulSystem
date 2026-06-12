/**
 * cosine_sim.cu — Batch cosine similarity via cuBLAS GEMM
 * 
 * output[i,j] = dot(normalize(query_i), normalize(db_j))
 * = (norm_q × norm_db^T)[i,j]
 * 
 * Row-major matrices, but cuBLAS uses column-major.
 * A_row[m,k] in col-major = A_col[k,m], so we need CUBLAS_OP_T for A.
 * B_row[n,k] transposed in col-major: we want B_row^T[k,n], 
 *   which in col-major is B_col = B_row^T = [k,n], so CUBLAS_OP_N.
 */
#include <cstdint>
#include <cuda_runtime.h>
#include <cublas_v2.h>

__global__ void l2_normalize_kernel(float* data, int rows, int cols) {
    int row = blockIdx.x * blockDim.x + threadIdx.x;
    if (row >= rows) return;
    float* row_ptr = data + row * cols;
    float sum_sq = 0.0f;
    for (int c = 0; c < cols; c++) sum_sq += row_ptr[c] * row_ptr[c];
    float inv_norm = 1.0f / (sqrtf(sum_sq) + 1e-8f);
    for (int c = 0; c < cols; c++) row_ptr[c] *= inv_norm;
}

extern "C" int launch_cutlass_cosine_sim(
    uint64_t queries_dev,
    uint64_t database_dev,
    uint64_t output_dev,
    uint32_t num_queries,
    uint32_t num_db,
    uint32_t dim,
    cudaStream_t stream
) {
    float* queries = reinterpret_cast<float*>(queries_dev);
    float* database = reinterpret_cast<float*>(database_dev);
    float* output = reinterpret_cast<float*>(output_dev);

    float* norm_q = nullptr;
    float* norm_db = nullptr;
    size_t bytes_q = (size_t)num_queries * dim * sizeof(float);
    size_t bytes_db = (size_t)num_db * dim * sizeof(float);

    if (cudaMallocAsync(&norm_q, bytes_q, stream) != cudaSuccess) return -1;
    if (cudaMallocAsync(&norm_db, bytes_db, stream) != cudaSuccess) {
        cudaFreeAsync(norm_q, stream); return -1;
    }

    cudaMemcpyAsync(norm_q, queries, bytes_q, cudaMemcpyDeviceToDevice, stream);
    cudaMemcpyAsync(norm_db, database, bytes_db, cudaMemcpyDeviceToDevice, stream);

    int block = 256;
    l2_normalize_kernel<<<(num_queries + block - 1) / block, block, 0, stream>>>(norm_q, num_queries, dim);
    l2_normalize_kernel<<<(num_db + block - 1) / block, block, 0, stream>>>(norm_db, num_db, dim);

    cublasHandle_t handle;
    if (cublasCreate(&handle) != CUBLAS_STATUS_SUCCESS) {
        cudaFreeAsync(norm_q, stream); cudaFreeAsync(norm_db, stream);
        return -2;
    }
    cublasSetStream(handle, stream);

    // C(m,n) = A^T * B  where A=norm_q[m,k], B=norm_db[n,k] (both row-major)
    // In cuBLAS column-major: A_col[k,m] with OP_T, B_col[k,n] with OP_N
    // Result C_col[m,n] → row-major C is C_col^T = same data, different view
    float alpha = 1.0f, beta = 0.0f;
    cublasStatus_t status = cublasSgemm(
        handle,
        CUBLAS_OP_T,    // A: transpose (row→col)
        CUBLAS_OP_N,    // B: no transpose (already col view)
        num_queries,    // m = rows of C
        num_db,         // n = cols of C
        dim,            // k = inner dimension
        &alpha,
        norm_q, dim,    // A (ld=dim, k×m in col view)
        norm_db, dim,   // B (ld=dim, k×n in col view)
        &beta,
        output, num_queries  // C (ld=m, m×n in col view)
    );

    cublasDestroy(handle);
    cudaFreeAsync(norm_q, stream);
    cudaFreeAsync(norm_db, stream);

    return (status == CUBLAS_STATUS_SUCCESS) ? 0 : -3;
}