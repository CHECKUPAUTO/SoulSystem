// ==========================================================================
// coherence.rs — Continuous Coherence Loss (Pillar 6)
//
// Pure function computing the Mean Squared Error between planned trajectory
// vectors and the perceiver's latent states.  Zero tokenisation — operates
// directly on the continuous latent space.
//
// Used as the closed-loop objective that drives adaptation.
// ==========================================================================

use candle_core::{Result, Tensor};

/// Compute the continuous coherence loss:
///
///   L_coherence = (1 / (T × d)) Σ_t Σ_i (trajectory[t][i] - latent[t][i])²
///
/// where `trajectory` is the planned path (T × d) and `latent` is the
/// perceiver output (T × d).
///
/// Returns the scalar loss and the per-timestep vector of MSE contributions
/// (useful for weighting the conditioning error e_t).
pub fn continuous_coherence_loss(
    trajectory: &Tensor,
    latent: &Tensor,
) -> Result<(f64, Vec<f64>)> {
    let shape_traj = trajectory.shape();
    let shape_lat = latent.shape();
    assert_eq!(shape_traj, shape_lat, "trajectory and latent must share shape");

    // Element-wise squared difference
    let diff = trajectory.sub(latent)?;
    let sq_err = diff.sqr()?;
    let mse_tensor = sq_err.mean_all()?;
    let mse: f64 = mse_tensor.to_scalar::<f64>()?;

    // Per-timestep MSE (mean over latent dimension)
    let per_step = sq_err.mean(1)?;  // (T,)
    let per_step_vec: Vec<f64> = per_step.to_vec1::<f64>()?;

    Ok((mse, per_step_vec))
}

/// Compute the absolute prediction error for a single timestep (used as e_t
/// input to the ConditioningHyperNetwork).
pub fn prediction_error(target: &Tensor, predicted: &Tensor) -> Result<f64> {
    let diff = target.sub(predicted)?;
    let abs_err = diff.abs()?.mean_all()?;
    Ok(abs_err.to_scalar::<f64>()?)
}
