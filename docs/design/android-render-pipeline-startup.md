# Android render-pipeline startup

The POCO M8 Phase 4 capture showed a cold-start stall in Bevy's
`upscaling::prepare_view_upscaling_pipelines`: 3.48 seconds across four calls. The
vendored Bevy implementation specializes each view's blit pipeline and calls
`PipelineCache::block_on_render_pipeline`, so a view can block the render schedule until
its key is compiled.

NVForever already pre-specializes all twelve blit keys. The previous warm-up system ran
in `RenderSystems::Prepare`, the same render phase as Bevy's blocking view preparation,
with no ordering relationship. The first camera could therefore reach the blocking
compile before warm-up had queued its keys.

Phase 5 moves warm-up into `RenderStartup`, explicitly after both the blit pipeline and
its `SpecializedRenderPipelines` cache are initialized. It runs before the first normal
render schedule and repeats when the render device is recovered.

The first four unblended formats are the ones used by the first views: the two possible
sRGB window formats, the reflection target (`Rgba16Float`), and the ripple target
(`R32Float`). Those four are compiled synchronously during render startup, before the
first view can block on them. The remaining two image formats and all six blended
variants are queued after them for background compilation. This shifts the measured
first-view pipeline compilation out of the first rendered frame while preserving all
twelve variants.

This does not change image quality, camera output, render scale, or presentation mode.
The unit tests check the first-view priority ordering and that all formats and blend
variants remain present. CI can verify compilation and unit tests; only a repeat POCO M8
run can establish the effect on Qualcomm's driver and startup duration.
