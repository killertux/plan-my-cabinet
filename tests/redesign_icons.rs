// Keep this target independently runnable while the parent integrates the module.
#[path = "../src/ui/icons.rs"]
mod icons;

use eframe::egui::{
    self, Color32, ImageSource, Vec2,
    load::{ImagePoll, SizeHint, TexturePoll},
};
use icons::{Icon, icon, install_loaders};

fn bytes(symbol: Icon) -> egui::load::Bytes {
    let ImageSource::Bytes { bytes, .. } = symbol.source() else {
        panic!("icon must be embedded");
    };
    bytes
}

#[test]
fn geometry_matches_original_handoff() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_dir = root.join("design_handoff_egui_redesign/assets/icons");
    let names: std::collections::BTreeSet<_> = Icon::ALL.map(Icon::name).into_iter().collect();
    let source_names: std::collections::BTreeSet<_> = std::fs::read_dir(source_dir.clone())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "svg"))
        .map(|path| path.file_stem().unwrap().to_str().unwrap().to_owned())
        .collect();
    assert_eq!(names.len(), 40);
    assert_eq!(
        names
            .into_iter()
            .map(str::to_owned)
            .collect::<std::collections::BTreeSet<_>>(),
        source_names
    );
    for symbol in Icon::ALL {
        let source =
            std::fs::read_to_string(source_dir.join(format!("{}.svg", symbol.name()))).unwrap();
        // Remove only the non-rendering credential envelope, whose signed bytes
        // remain authoritative in the unmodified handoff, then the stroke color.
        let (before, rest) = source.split_once("<metadata>").expect("source provenance");
        let (_, after) = rest.split_once("</metadata>").unwrap();
        let expected = format!("{before}{after}")
            .replace(" xmlns:c2pa=\"http://c2pa.org/manifest\"", "")
            .replace("stroke=\"#000\"", "stroke=\"#fff\"");
        assert_eq!(
            std::str::from_utf8(&bytes(symbol)).unwrap().trim(),
            expected.trim(),
            "{symbol:?}: all geometry and stroke attributes must match"
        );
    }
}

#[test]
fn embedded_assets_resolve_and_rasterize_at_every_reference_size() {
    let ctx = egui::Context::default();
    install_loaders(&ctx);
    install_loaders(&ctx);
    for symbol in Icon::ALL {
        for size in 11..=19 {
            for pixels_per_point in [1.0, 2.0] {
                ctx.set_pixels_per_point(pixels_per_point);
                ctx.run_ui(Default::default(), |_| {})
                    .textures_delta
                    .clear();
                let image = icon(symbol, Color32::WHITE, size as f32);
                let TexturePoll::Ready { texture } =
                    image.load_for_size(&ctx, Vec2::splat(size as f32)).unwrap()
                else {
                    panic!("embedded {symbol:?} must load synchronously");
                };
                let physical = size as f32 * pixels_per_point;
                // SizedTexture retains the SVG's logical source size, while
                // the decoded raster below must have the requested pixel size.
                assert_eq!(texture.size, Vec2::splat(24.0));
                assert_eq!(
                    image.calc_size(Vec2::splat(100.0), Some(texture.size)),
                    Vec2::splat(size as f32)
                );
                let ImagePoll::Ready { image: raster } = ctx
                    .try_load_image(
                        image.uri().unwrap(),
                        SizeHint::Size {
                            width: physical as u32,
                            height: physical as u32,
                            maintain_aspect_ratio: true,
                        },
                    )
                    .unwrap()
                else {
                    panic!("missing raster");
                };
                assert_eq!(raster.size, [physical as usize; 2]);
                assert!(
                    raster.pixels.iter().any(|p| p.a() > 32),
                    "{symbol:?} is blank at {size}"
                );
                assert!(
                    raster.pixels.iter().any(|p| p.a() == 0),
                    "{symbol:?} lost transparency"
                );
                for pixel in raster.pixels.iter().filter(|p| p.a() > 0) {
                    assert_eq!(
                        pixel.to_srgba_unmultiplied()[..3],
                        [255; 3],
                        "{symbol:?} is not a white mask"
                    );
                }
            }
        }
    }
}

/// Actual egui-wgpu shader, texture upload, blend, and GPU readback. Explicitly
/// opt in on machines with a graphics adapter; never silently skip a failure.
#[test]
#[ignore = "requires a native wgpu adapter; run --ignored for raster proof"]
fn gpu_raster_proves_all_icons_and_state_colors() {
    use eframe::{egui_wgpu, wgpu};
    const WIDTH: u32 = 1152; // 256-byte-aligned RGBA rows
    const HEIGHT: u32 = 960;
    const CELL: usize = 24;
    const STATE_WIDTH: usize = 9 * CELL;
    let colors = [
        Color32::WHITE,                                         // mask control
        Color32::from_rgb(0x5a, 0x52, 0x48),                    // normal text_3
        Color32::from_rgb(0xc9, 0x73, 0x1f),                    // selected accent
        Color32::from_rgb(0xb7, 0x79, 0x1f),                    // warning
        Color32::from_rgba_unmultiplied(0x5a, 0x52, 0x48, 102), // disabled 40%
    ];
    let ctx = egui::Context::default();
    install_loaders(&ctx);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(WIDTH as f32, HEIGHT as f32),
            )),
            ..Default::default()
        },
        |ui| {
            for (row, symbol) in Icon::ALL.into_iter().enumerate() {
                for (state, color) in colors.into_iter().enumerate() {
                    for size in 11..=19 {
                        let position = egui::pos2(
                            (state * STATE_WIDTH + (size - 11) * CELL + 2) as f32,
                            (row * CELL + 2) as f32,
                        );
                        icon(symbol, color, size as f32).paint_at(
                            ui,
                            egui::Rect::from_min_size(position, Vec2::splat(size as f32)),
                        );
                    }
                }
            }
        },
    );
    let jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("native graphics adapter");
    eprintln!("icon raster adapter: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("icon raster proof"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut renderer = egui_wgpu::Renderer::new(
        &device,
        format,
        egui_wgpu::RendererOptions {
            dithering: false,
            ..Default::default()
        },
    );
    for (id, deltas) in &output.textures_delta.set {
        for delta in deltas {
            renderer.update_texture(&device, &queue, *id, delta);
        }
    }
    output.textures_delta.clear();
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [WIDTH, HEIGHT],
        pixels_per_point: 1.0,
    };
    let mut encoder = device.create_command_encoder(&Default::default());
    let callbacks = renderer.update_buffers(&device, &queue, &mut encoder, &jobs, &screen);
    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("icons"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            })
            .forget_lifetime();
        renderer.render(&mut pass, &jobs, &screen);
    }
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("icon readback"),
        size: u64::from(WIDTH * HEIGHT * 4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(WIDTH * 4),
                rows_per_image: Some(HEIGHT),
            },
        },
        target.size(),
    );
    queue.submit(callbacks.into_iter().chain([encoder.finish()]));
    let (sender, receiver) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap()
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver.recv().unwrap().unwrap();
    let pixels = buffer.slice(..).get_mapped_range().unwrap();
    let pixel = |x: usize, y: usize| -> &[u8] {
        let offset = (y * WIDTH as usize + x) * 4;
        &pixels[offset..offset + 4]
    };
    for (row, symbol) in Icon::ALL.into_iter().enumerate() {
        for size in 11..=19 {
            let x0 = (size - 11) * CELL;
            let y0 = row * CELL;
            let mut ink = 0;
            for y in y0..y0 + CELL {
                for x in x0..x0 + CELL {
                    let mask = pixel(x, y);
                    if mask[3] > 32 {
                        ink += 1;
                        assert!(mask[0] > 32, "black source: {symbol:?}/{size}");
                    }
                    for (state, color) in colors.iter().enumerate().skip(1) {
                        let actual = pixel(x + state * STATE_WIDTH, y);
                        for channel in 0..4 {
                            let expected = f32::from(mask[channel])
                                * f32::from(color.to_array()[channel])
                                / 255.0;
                            assert!(
                                (f32::from(actual[channel]) - expected).abs() <= 2.0,
                                "{symbol:?}/{size} state {state} channel {channel}: {actual:?}, mask {mask:?}, expected {expected}"
                            );
                        }
                    }
                }
            }
            assert!(ink > 0, "blank GPU icon {symbol:?}/{size}");
        }
    }
    // Optional offline contact sheet, preserving an easy-to-inspect artifact.
    // Columns: white control, normal, selected, warning, disabled; sizes 11..19.
    if let Some(path) = std::env::var_os("REDESIGN_ICON_RASTER_PPM") {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        write!(file, "P6\n{WIDTH} {HEIGHT}\n255\n").unwrap();
        for rgba in pixels.as_chunks::<4>().0 {
            let bg = [0xf4_u32, 0xf1, 0xec];
            let rgb = std::array::from_fn::<_, 3, _>(|i| {
                (u32::from(rgba[i]) + bg[i] * (255 - u32::from(rgba[3])) / 255).min(255) as u8
            });
            file.write_all(&rgb).unwrap();
        }
    }
}
