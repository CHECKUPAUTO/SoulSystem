/**
 * hnn_step.cu — HNN Verlet integration step
 */
#include <cstdint>
#include <cuda_runtime.h>
#include <cmath>

__global__ void hnn_verlet_kernel(
    float* q, float* p, const float* masses,
    float alpha, float beta, float dt,
    uint32_t num_trajectories, uint32_t dim
) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    int total = num_trajectories * dim;
    if (idx >= total) return;

    int traj = idx / dim;
    float qi = q[idx], pi = p[idx];
    float inv_m = 1.0f / masses[traj];
    float half_dt = 0.5f * dt;

    // Half-step momentum
    float dq = qi;
    float force = -2.0f * alpha * dq - 4.0f * beta * dq * dq * dq;
    pi += half_dt * force * inv_m;

    // Full-step position
    qi += dt * pi * inv_m;

    // Recompute force at new position
    dq = qi;
    float force_new = -2.0f * alpha * dq - 4.0f * beta * dq * dq * dq;
    pi += half_dt * force_new * inv_m;

    q[idx] = qi;
    p[idx] = pi;
}

extern "C" int launch_hnn_verlet_step(
    uint64_t q_dev, uint64_t p_dev, uint64_t masses_dev,
    float alpha, float beta, float dt,
    uint32_t num_trajectories, uint32_t dim,
    cudaStream_t stream
) {
    float* q = reinterpret_cast<float*>(q_dev);
    float* p = reinterpret_cast<float*>(p_dev);
    const float* masses = reinterpret_cast<const float*>(masses_dev);

    int total = num_trajectories * dim;
    int block = 256;
    int grid = (total + block - 1) / block;

    hnn_verlet_kernel<<<grid, block, 0, stream>>>(
        q, p, masses, alpha, beta, dt, num_trajectories, dim);
    return (cudaGetLastError() == cudaSuccess) ? 0 : -1;
}