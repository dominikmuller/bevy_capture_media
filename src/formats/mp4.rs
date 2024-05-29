use std::path::PathBuf;
use std::time::Duration;

use crossbeam_channel::Sender;

use ndarray::Array3;
use video_rs::encode::{Encoder, Settings};
use video_rs::time::Time;

use bevy::prelude::*;
use bevy::render::texture::TextureFormatPixelInfo;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use futures_lite::future;

use crate::data::{ActiveRecorders, CaptureFrame, HasTaskStatus};

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Default, Event)]
pub enum Mp4State {
    #[default]
    Start,
    Stop,
}

#[derive(Debug, Clone)]
pub enum Mp4TaskPayload {
    Data(Duration, Vec<u8>),
    Terminate,
}

pub type Mp4Capture = CaptureFrame<Mp4State>;

#[derive(Component)]
pub struct Mp4Task(pub Task<()>, pub Sender<Mp4TaskPayload>, pub usize);

impl HasTaskStatus for Mp4Task {
    fn is_done(&mut self) -> bool {
        let result = future::block_on(future::poll_once(&mut self.0));
        result.is_some()
    }
}

pub fn manage_mp4_task(
    mut commands: Commands,
    mut events: ResMut<Events<Mp4Capture>>,
    recorders: ResMut<ActiveRecorders>,
    images: Res<Assets<Image>>,
    tasks: Query<&Mp4Task>,
) {
    let thread_pool = AsyncComputeTaskPool::get();
    'event_drain: for event in events.drain() {
        if let Some(recorder) = recorders.get(&event.tracking_id) {
            let (width, height, target_format) = match images.get(&recorder.target_handle) {
                Some(image) => (
                    image.size().x,
                    image.size().y,
                    image.texture_descriptor.format,
                ),
                None => continue 'event_drain,
            };
            // Channel used to transfer frames and termination signal to the worker task
            let (sender, receiver) = crossbeam_channel::unbounded::<Mp4TaskPayload>();

            match event.capture_type {
                Mp4State::Start => {
                    let task = thread_pool.spawn(async move {
                        let even_width = ((width as f64 / 2.0).ceil() * 2.0) as usize;

                        let file_name = event.path.unwrap_or_else(|| {
                            PathBuf::from(format!(
                                "{}.mp4",
                                std::time::UNIX_EPOCH.elapsed().unwrap().as_secs()
                            ))
                        });
                        let settings =
                            Settings::preset_h264_yuv420p(even_width, height as usize, false);
                        let mut encoder =
                            Encoder::new(file_name, settings).expect("failed to create encoder");

                        let format = target_format;
                        let mut position = Time::zero();
                        while let Ok(Mp4TaskPayload::Data(frame_time, frame_data)) = receiver.recv()
                        {
                            let expected_size = width * height * format.pixel_size() as u32;
                            if expected_size != frame_data.len() as u32 {
                                log::error!(
                                    "Failed to assert that the data frame is correctly formatted"
                                );
                                return;
                            }
                            let frame = Array3::from_shape_fn(
                                (height as usize, even_width, 3),
                                |(h, w, c)| {
                                    if w >= width as usize {
                                        0
                                    } else {
                                        frame_data[w * format.pixel_size()
                                            + format.pixel_size() * h * width as usize
                                            + c]
                                    }
                                },
                            );
                            encoder
                                .encode(&frame, position)
                                .expect("failed to encode frame");

                            // Update the current position and add the inter-frame duration to it.
                            position = position.aligned_with(frame_time.into()).add();
                        }
                        encoder.finish().expect("failed to finish encoder");
                    });

                    commands.spawn(Mp4Task(task, sender, event.tracking_id));
                }
                Mp4State::Stop => {
                    if let Some(task) = tasks.iter().find(|t| t.2 == event.tracking_id) {
                        task.1
                            .send(Mp4TaskPayload::Terminate)
                            .expect("Failed to send terminate signal to task");
                    }
                }
            }
        }
    }
}

pub fn send_frame_to_mp4_tasks(recorders: ResMut<ActiveRecorders>, tasks: Query<&Mp4Task>) {
    for task in tasks.iter() {
        if let Some(recorder) = recorders.get(&task.2) {
            let (frame_duration, frame_data) = match recorder.frames.back() {
                Some(data) => (data.frame_time, data.texture.clone()),
                None => continue,
            };
            let _ = task
                .1
                .send(Mp4TaskPayload::Data(frame_duration, frame_data));
            //.expect("Failed to send data to task");
        }
    }
}
