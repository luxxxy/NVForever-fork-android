//! The upscaling pass's pipelines compiled at start-up, so a camera that switches on later
//! does not stall the frame.
//!
//! Bevy's upscaling pass (`bevy_core_pipeline` 0.19.1 `upscaling/mod.rs`,
//! `prepare_view_upscaling_pipelines`) blocks the frame until a view's blit pipeline is
//! compiled, the first time a view with a new key (output format, blend, source space)
//! is drawn ("ensure the pipeline is loaded before continuing the frame") [documented].
//! The compile is a task on the async compute pool, queued behind every other pipeline
//! being compiled, so when the camera switches on as the player comes into a new place,
//! with the place's own shaders compiling, the frame waits for all of them: 1.3 s on the
//! way into Primm, where the water reflection camera (reflection.rs) switches on, a
//! `profile hitch` line with `upscaling::prepare_view_upscaling_pipelines` 1305 ms in it
//! [observed: tools/stress tour Goodsprings–Primm, 2026-10-27; docs/design/
//! streaming-hitches.md §3 had the frame as "not looked into yet"].
//!
//! Every key the game's cameras can ask for is specialized once on the render app's first
//! frames ([`KEYS`]), without waiting: the compiles finish under the loading screen and
//! a later camera finds its pipeline ready. The formats are those of the window (Bevy
//! names a window's Bgra8UnormSrgb Rgba8UnormSrgb) and of the image targets
//! (Bgra8UnormSrgb for the Pip-Boy, terminal, scope and table views, Rgba16Float for the
//! reflection, R32Float for the ripples); a camera after the first on a target blends
//! (alpha blending); no camera sets a compositing space [engineering choice: a dozen small
//! full-screen pipelines].

use bevy::core_pipeline::blit::{BlitPipeline, BlitPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{BlendState, PipelineCache, SpecializedRenderPipelines, TextureFormat};
use bevy::render::{RenderApp, RenderStartup};

/// The output formats of the game's views.
const FORMATS: [TextureFormat; 6] = [
    TextureFormat::Rgba8UnormSrgb,
    TextureFormat::Bgra8UnormSrgb,
    TextureFormat::Rgba16Float,
    TextureFormat::R32Float,
    TextureFormat::Rgba8Unorm,
    TextureFormat::Bgra8Unorm,
];

/// Every upscaling key warmed: each format, written over (the first camera on a target)
/// and blended (the ones after it).
pub fn keys() -> Vec<BlitPipelineKey> {
    let mut keys = Vec::with_capacity(FORMATS.len() * 2);

    // The main window uses one of the first two formats. The reflection and ripple
    // targets use the next two. Keep all four first-view formats ahead of secondary
    // unorm targets and blended variants.
    for &target_format in &FORMATS[..2] {
        keys.push(BlitPipelineKey { hdr: false, hdr_gamma22: false, target_format, blend_state: None, samples: 1, source_space: None });
    }
    for &target_format in &FORMATS[2..] {
        keys.push(BlitPipelineKey { hdr: false, hdr_gamma22: false, target_format, blend_state: None, samples: 1, source_space: None });
    }
    for &target_format in &FORMATS {
        keys.push(BlitPipelineKey { hdr: false, hdr_gamma22: false, target_format, blend_state: Some(BlendState::ALPHA_BLENDING), samples: 1, source_space: None });
    }
    keys
}

pub struct BlitWarmPlugin;

impl Plugin for BlitWarmPlugin {
    fn build(&self, app: &mut App) {
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            // The first view blocks in Bevy's prepare_view_upscaling_pipelines until
            // its blit pipeline is ready. Queue the window variants during RenderStartup,
            // after Bevy has created both resources, rather than racing that blocking
            // system in RenderSystems::Prepare. RenderStartup runs again on device recovery.
            render.add_systems(
                RenderStartup,
                warm
                    .after(bevy::core_pipeline::blit::init_blit_pipeline)
                    .after(bevy::render::init_gpu_resource::<SpecializedRenderPipelines<BlitPipeline>>),
            );
        }
    }
}

/// Specializes every key after the blit pipeline and specialized-pipeline cache have
/// been initialized for this render device.
fn warm(
    mut cache: ResMut<PipelineCache>,
    blit: Res<BlitPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<BlitPipeline>>,
) {
    let keys = keys();

    // These are the window, reflection and ripple formats used by the first views.
    // Compile them now, before Bevy's first-view preparation can block on them. The
    // remaining image formats and blended variants are queued for background compilation.
    for key in keys.iter().take(4) {
        let id = pipelines.specialize(&cache, &blit, *key);
        cache.block_on_render_pipeline(id);
    }
    for key in keys.iter().skip(4) {
        pipelines.specialize(&cache, &blit, *key);
    }
    info!(
        "blit warm: 4 first-view pipelines ready; {} secondary variants queued during render startup",
        keys.len() - 4
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_window_srgb_variants_are_first_and_unblended() {
        let k = keys();
        assert_eq!(k[0].target_format, TextureFormat::Rgba8UnormSrgb);
        assert_eq!(k[1].target_format, TextureFormat::Bgra8UnormSrgb);
        assert!(k[0].blend_state.is_none());
        assert!(k[1].blend_state.is_none());
    }

    #[test]
    fn every_format_written_and_blended() {
        let k = keys();
        assert_eq!(k.len(), FORMATS.len() * 2);
        let reflection = BlitPipelineKey { hdr: false, hdr_gamma22: false, target_format: TextureFormat::Rgba16Float, blend_state: None, samples: 1, source_space: None };
        assert!(k.contains(&reflection), "the water reflection camera's key");
    }
}
