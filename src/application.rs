use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState, SubpassContents};
use vulkano::device::{Device, DeviceExtensions};
use vulkano::framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass};
use vulkano::image::{ImageUsage, SwapchainImage};
use vulkano::instance::{Instance, PhysicalDevice};
use vulkano::pipeline::viewport::Viewport;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::swapchain;
use vulkano::swapchain::{
    AcquireError, ColorSpace, FullscreenExclusive, PresentMode, SurfaceTransform, Swapchain,
    SwapchainCreationError,
};
use vulkano::sync;
use vulkano::sync::{FlushError, GpuFuture};

use vulkano_win::VkSurfaceBuild;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

use std::sync::Arc;

/* Create a vertex type to represent vertices. */
#[derive(Default, Debug, Clone)]
struct Vertex {
    position: [f32; 2],
}
vulkano::impl_vertex!(Vertex, position);

pub struct Application<'a> {
    instance: Arc<Instance>,
    physical: PhysicalDevice<'a>,
}

impl<'a> Application<'a> {
    pub fn new() -> Application<'a> {
        /* Retrieve extensions needed for a Vulkan window. */
        let required_extensions = vulkano_win::required_extensions();
        /* Create a Vulkan instance. */
        let instance = Instance::new(None, &required_extensions, None).unwrap();
        /* Retrieve the physical device (GPU). */
        let physical = PhysicalDevice::enumerate(&instance).next().unwrap();
        /* Debug */
        println!(
            "Using device: {} (type: {:?})",
            physical.name(),
            physical.ty()
        );

        /* Create a surface for Vulkan to draw to. */
        let event_loop = EventLoop::new();
        let surface = WindowBuilder::new()
            .build_vk_surface(&event_loop, instance.clone())
            .unwrap();

        /* Grab GPU queues that support graphics and the current window surface */
        let queue_family = physical
            .queue_families()
            .find(|&q| q.supports_graphics() && surface.is_supported(q).unwrap_or(false))
            .unwrap();

        /* Retrieve the Vulkan device along with it's queues. It must allow
         * for a swapchain (Think double buffering) */
        let device_ext = DeviceExtensions {
            khr_swapchain: true,
            ..DeviceExtensions::none()
        };
        let (device, mut queues) = Device::new(
            physical,
            physical.supported_features(),
            &device_ext,
            /* Priority of 0.5. */
            [(queue_family, 0.5)].iter().cloned(),
        )
        .unwrap();

        /* Take the 1st GPU queue */
        let queue = queues.next().unwrap();

        let (mut swapchain, images) = {
            /* Grab the surface's capabilities. */
            let caps = surface.capabilities(physical).unwrap();

            let alpha = caps.supported_composite_alpha.iter().next().unwrap();
            let format = caps.supported_formats[0].0;
            let dimensions: [u32; 2] = surface.window().inner_size().into();
            /* Create the swapchain */
            Swapchain::new(
                device.clone(),
                surface.clone(),
                caps.min_image_count,
                format,
                dimensions,
                1,
                ImageUsage::color_attachment(),
                &queue,
                SurfaceTransform::Identity,
                alpha,
                PresentMode::Fifo,
                FullscreenExclusive::Default,
                true,
                ColorSpace::SrgbNonLinear,
            )
            .unwrap()
        };

        /* Create a vertex buffer representing the lower triangle of the screen. */
        let upper_tri = CpuAccessibleBuffer::from_iter(
            device.clone(),
            BufferUsage::all(),
            false,
            [
                Vertex {
                    position: [-1.0, -1.0],
                },
                Vertex {
                    position: [1.0, 1.0],
                },
                Vertex {
                    position: [-1.0, 1.0],
                },
            ]
            .iter()
            .cloned(),
        )
        .unwrap();

        let lower_tri = CpuAccessibleBuffer::from_iter(
            device.clone(),
            BufferUsage::all(),
            false,
            [
                Vertex {
                    position: [-1.0, -1.0],
                },
                Vertex {
                    position: [1.0, 1.0],
                },
                Vertex {
                    position: [1.0, -1.0],
                },
            ]
            .iter()
            .cloned(),
        )
        .unwrap();

        /* Load fragment and vertex shaders. */
        let vs = vs::Shader::load(device.clone()).unwrap();
        let fs = fs::Shader::load(device.clone()).unwrap();

        let render_pass = Arc::new(
            vulkano::single_pass_renderpass!(
                device.clone(),
                attachments: {
                    color: {
                        /* Clear the draw when first starting. */
                        load: Clear,
                        /* Store the draw output in an image. */
                        store: Store,
                        /* Make the format the same as that of the swapchain. */
                        format: swapchain.format(),
                        samples: 1,
                    }
                },
                pass: {
                    /* We use the attachment named `color` as the one and only color attachment. */
                    color: [color],
                    /* No depth-stencil attachment is indicated with empty brackets. */
                    depth_stencil: {}
                }
            )
            .unwrap(),
        );

        /* Make the graphics pipeline. */
        let pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer()
                .vertex_shader(vs.main_entry_point(), ())
                /* The content of the vertex buffer describes a list of triangles. */
                .triangle_list()
                /* Use a resizable viewport set to draw over the entire window */
                .viewports_dynamic_scissors_irrelevant(1)
                // See `vertex_shader`.
                .fragment_shader(fs.main_entry_point(), ())
                // We have to indicate which subpass of which render pass this pipeline is going to be used
                // in. The pipeline will only be usable from this particular subpass.
                .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
                // Now that our builder is filled, we call `build()` to obtain an actual pipeline.
                .build(device.clone())
                .unwrap(),
        );

        let mut dynamic_state = DynamicState {
            line_width: None,
            viewports: None,
            scissors: None,
            compare_mask: None,
            write_mask: None,
            reference: None,
        };

        /* Create framebuffer for each image. */
        let mut framebuffers =
            window_size_dependent_setup(&images, render_pass.clone(), &mut dynamic_state);

        // Initialization is finally finished!

        /* swapchain can become invalid so if it does, it needs to be recreated.
         * This boolean keeps track of whether that is the case. */
        let mut recreate_swapchain = false;

        /* In the loop below we are going to submit commands to the GPU. Submitting a command produces
         * an object that implements the `GpuFuture` trait, which holds the resources for as long as
         * they are in use by the GPU.
         * Destroying the `GpuFuture` blocks until the GPU is finished executing it. In order to avoid
         * that, we store the submission of the previous frame here. */
        let mut previous_frame_end = Some(sync::now(device.clone()).boxed());

        event_loop.run(move |event, _, control_flow| {
            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    *control_flow = ControlFlow::Exit;
                }
                Event::WindowEvent {
                    event: WindowEvent::Resized(_),
                    ..
                } => {
                    recreate_swapchain = true;
                }
                Event::RedrawEventsCleared => {
                    // It is important to call this function from time to time, otherwise resources will keep
                    // accumulating and you will eventually reach an out of memory error.
                    // Calling this function polls various fences in order to determine what the GPU has
                    // already processed, and frees the resources that are no longer needed.
                    previous_frame_end.as_mut().unwrap().cleanup_finished();

                    // Whenever the window resizes we need to recreate everything dependent on the window size.
                    // In this example that includes the swapchain, the framebuffers and the dynamic state viewport.
                    if recreate_swapchain {
                        // Get the new dimensions of the window.
                        let dimensions: [u32; 2] = surface.window().inner_size().into();
                        let (new_swapchain, new_images) =
                            match swapchain.recreate_with_dimensions(dimensions) {
                                Ok(r) => r,
                                // This error tends to happen when the user is manually resizing the window.
                                // Simply restarting the loop is the easiest way to fix this issue.
                                Err(SwapchainCreationError::UnsupportedDimensions) => return,
                                Err(e) => panic!("Failed to recreate swapchain: {:?}", e),
                            };

                        swapchain = new_swapchain;
                        // Because framebuffers contains an Arc on the old swapchain, we need to
                        // recreate framebuffers as well.
                        framebuffers = window_size_dependent_setup(
                            &new_images,
                            render_pass.clone(),
                            &mut dynamic_state,
                        );
                        recreate_swapchain = false;
                    }

                    // Before we can draw on the output, we have to *acquire* an image from the swapchain. If
                    // no image is available (which happens if you submit draw commands too quickly), then the
                    // function will block.
                    // This operation returns the index of the image that we are allowed to draw upon.
                    //
                    // This function can block if no image is available. The parameter is an optional timeout
                    // after which the function call will return an error.
                    let (image_num, suboptimal, acquire_future) =
                        match swapchain::acquire_next_image(swapchain.clone(), None) {
                            Ok(r) => r,
                            Err(AcquireError::OutOfDate) => {
                                recreate_swapchain = true;
                                return;
                            }
                            Err(e) => panic!("Failed to acquire next image: {:?}", e),
                        };

                    // acquire_next_image can be successful, but suboptimal. This means that the swapchain image
                    // will still work, but it may not display correctly. With some drivers this can be when
                    // the window resizes, but it may not cause the swapchain to become out of date.
                    if suboptimal {
                        recreate_swapchain = true;
                    }

                    // Specify the color to clear the framebuffer with i.e. blue
                    let clear_values = vec![[0.0, 0.0, 1.0, 1.0].into()];

                    // In order to draw, we have to build a *command buffer*. The command buffer object holds
                    // the list of commands that are going to be executed.
                    //
                    // Building a command buffer is an expensive operation (usually a few hundred
                    // microseconds), but it is known to be a hot path in the driver and is expected to be
                    // optimized.
                    //
                    // Note that we have to pass a queue family when we create the command buffer. The command
                    // buffer will only be executable on that given queue family.
                    let mut builder = AutoCommandBufferBuilder::primary_one_time_submit(
                        device.clone(),
                        queue.family(),
                    )
                    .unwrap();

                    builder
                        // Before we can draw, we have to *enter a render pass*. There are two methods to do
                        // this: `draw_inline` and `draw_secondary`. The latter is a bit more advanced and is
                        // not covered here.
                        //
                        // The third parameter builds the list of values to clear the attachments with. The API
                        // is similar to the list of attachments when building the framebuffers, except that
                        // only the attachments that use `load: Clear` appear in the list.
                        .begin_render_pass(
                            framebuffers[image_num].clone(),
                            SubpassContents::Inline,
                            clear_values,
                        )
                        .unwrap()
                        // We are now inside the first subpass of the render pass. We add a draw command.
                        //
                        // The last two parameters contain the list of resources to pass to the shaders.
                        // Since we used an `EmptyPipeline` object, the objects have to be `()`.
                        .draw(pipeline.clone(), &dynamic_state, upper_tri.clone(), (), ())
                        .unwrap()
                        .draw(pipeline.clone(), &dynamic_state, lower_tri.clone(), (), ())
                        .unwrap()
                        // We leave the render pass by calling `draw_end`. Note that if we had multiple
                        // subpasses we could have called `next_inline` (or `next_secondary`) to jump to the
                        // next subpass.
                        .end_render_pass()
                        .unwrap();

                    // Finish building the command buffer by calling `build`.
                    let command_buffer = builder.build().unwrap();

                    let future = previous_frame_end
                        .take()
                        .unwrap()
                        .join(acquire_future)
                        .then_execute(queue.clone(), command_buffer)
                        .unwrap()
                        // The color output is now expected to contain our triangle. But in order to show it on
                        // the screen, we have to *present* the image by calling `present`.
                        //
                        // This function does not actually present the image immediately. Instead it submits a
                        // present command at the end of the queue. This means that it will only be presented once
                        // the GPU has finished executing the command buffer that draws the triangle.
                        .then_swapchain_present(queue.clone(), swapchain.clone(), image_num)
                        .then_signal_fence_and_flush();

                    match future {
                        Ok(future) => {
                            previous_frame_end = Some(future.boxed());
                        }
                        Err(FlushError::OutOfDate) => {
                            recreate_swapchain = true;
                            previous_frame_end = Some(sync::now(device.clone()).boxed());
                        }
                        Err(e) => {
                            println!("Failed to flush future: {:?}", e);
                            previous_frame_end = Some(sync::now(device.clone()).boxed());
                        }
                    }
                }
                _ => (),
            }
        });

        unimplemented!();
    }
}

/* Vertex and fragment shaders */
mod vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        src: "
			#version 450
			layout(location = 0) in vec2 position;
			void main() {
				gl_Position = vec4(position, 0.0, 1.0);
			}
		"
    }
}

mod fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: "
			#version 450
			layout(location = 0) out vec4 f_color;
			void main() {
				f_color = vec4(1.0, 0.0, 0.0, 1.0);
			}
		"
    }
}

/* Called once during initialization and again whenever the window is resized */
fn window_size_dependent_setup(
    images: &[Arc<SwapchainImage<Window>>],
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
    dynamic_state: &mut DynamicState,
) -> Vec<Arc<dyn FramebufferAbstract + Send + Sync>> {
    let dimensions = images[0].dimensions();

    let viewport = Viewport {
        origin: [0.0, 0.0],
        dimensions: [dimensions[0] as f32, dimensions[1] as f32],
        depth_range: 0.0..1.0,
    };
    dynamic_state.viewports = Some(vec![viewport]);

    images
        .iter()
        .map(|image| {
            Arc::new(
                Framebuffer::start(render_pass.clone())
                    .add(image.clone())
                    .unwrap()
                    .build()
                    .unwrap(),
            ) as Arc<dyn FramebufferAbstract + Send + Sync>
        })
        .collect::<Vec<_>>()
}
