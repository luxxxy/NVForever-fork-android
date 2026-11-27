# Android render-pipeline startup

The POCO M8 Phase 4 capture showed a cold-start stall in Bevy's
`upscaling::prepare_view_upscaling_pipelines`: 3.48 seconds across four calls.
The vendored Bevy implementation specializes the view's blit pipeline and calls
`PipelineCache::block_on_render_pipeline`, so the first camera can block the render
schedule until its key is compiled.

NVForever already pre-specializes all twelve blit keys. The previous warm-up system ran
in `RenderSystems::Prepare`, the same render phase as Bevy's blocking view preparation,
with no ordering relationship. The first camera could therefore reach the blocking
compile before the warm-up had queued its keys.

Phase 5 moves warm-up into `RenderStartup`, explicitly after both the blit pipeline and
its `SpecializedRenderPipelines` cache are initialized. This happens before the first
normal render schedule and is repeated when the render device is recovered. The two
possible sRGB window formats are queued first, unblended, because the surface setup
prefers sRGB and the first camera writing to the window does not blend. Off-screen
formats and blended variants follow.

This preserves every existing pipeline variant and does not change image quality,
camera output, render scale, or presentation mode. The unit test checks the priority
ordering and the existing test continues to check all twelve variants. CI can verify
compilation and unit tests; only a repeat POCO M8 run can establish whether the startup
stall is reduced on the Qualcomm driver.
