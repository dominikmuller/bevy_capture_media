#![cfg_attr(docsrs, feature(doc_auto_cfg))]

#[allow(clippy::type_complexity)]
pub mod data;
pub mod formats;
#[cfg(any(feature = "gif", feature = "png"))]
mod image_utils;
mod management;
mod render;
#[cfg(target_arch = "wasm32")]
mod web_utils;

mod plugin {
    use bevy::app::{App, Plugin};
    use bevy::prelude::*;
    use bevy::render::{Render, RenderApp, RenderSet};

    use super::*;

    pub struct BevyCapturePlugin;
    impl Plugin for BevyCapturePlugin {
        fn build(&self, app: &mut App) {
            let tracking_tracker = data::ActiveRecorders::default();
            let data_smuggler = data::SharedDataSmuggler::default();

            app.add_event::<data::StartTrackingCamera>()
                .add_event::<data::StopTrackingCamera>()
                .insert_resource(tracking_tracker)
                .insert_resource(data_smuggler.clone())
                .add_systems(PreUpdate, management::clean_cameras)
                .add_systems(PreUpdate, management::move_camera_buffers)
                .add_systems(PostUpdate, management::sync_tracking_cameras)
                .add_systems(PostUpdate, management::start_tracking_orthographic_camera);

            #[cfg(feature = "gif")]
            {
                app.add_event::<formats::gif::CaptureGifRecording>()
                    .add_systems(PostUpdate, formats::gif::capture_gif_recording);

                #[cfg(not(target_arch = "wasm32"))]
                app.add_systems(
                    Last,
                    management::clean_unmonitored_tasks::<formats::gif::SaveGifRecording>,
                );
            }
            #[cfg(feature = "png")]
            {
                app.add_event::<formats::png::SavePngFile>()
                    .add_systems(PostUpdate, formats::png::save_single_frame);

                #[cfg(not(target_arch = "wasm32"))]
                app.add_systems(
                    PostUpdate,
                    management::clean_unmonitored_tasks::<formats::png::SaveFrameTask>,
                );
            }
            #[cfg(feature = "mp4")]
            {
                app.add_event::<formats::mp4::Mp4Capture>()
                    .add_systems(PostUpdate, formats::mp4::manage_mp4_task)
                    .add_systems(PostUpdate, formats::mp4::send_frame_to_mp4_tasks);

                app.add_systems(
                    PostUpdate,
                    management::clean_unmonitored_tasks::<formats::mp4::Mp4Task>,
                );
            }

            let render_app = app.get_sub_app_mut(RenderApp)
				.expect("bevy_capture_media will not work without the render app. Either enable this sub app, or disable bevy_capture_media");

            render_app
                .insert_resource(data_smuggler)
                .add_systems(Render, render::smuggle_frame.in_set(RenderSet::Render));
        }
    }
}

pub use data::MediaCapture;
pub use plugin::BevyCapturePlugin;
