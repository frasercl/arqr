
use std::{thread, sync::mpsc, path::Path};
use image::{ImageBuffer, buffer::ConvertBuffer};
use nokhwa::Camera;
use piston_window::{
    PistonWindow,
    Texture,
    TextureSettings,
    WindowSettings,
    Glyphs,
    text::Text,
    Transformed,
};
use arqr::{scan, ScanResult};

const FPS: u32 = 30;
const SCAN_INTERVAL: u32 = 2;
const LINE_COLOR: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn main() {
    let mut cam = Camera::new(0, None).unwrap();
    let res = cam.resolution();
    let width = res.width();
    let height = res.height();
    
    // CAM THREAD gets frames from the camera
    let (cam_tx, cam_rx) = mpsc::channel();
    let (scan_tx, scan_rx) = mpsc::channel();
    let cam_thread = thread::spawn(move || {
        cam.set_frame_rate(FPS).unwrap();
        cam.open_stream().unwrap();
        let mut frame_counter = 0;

        let mut send_result = Ok(());
        while send_result.is_ok() {
            let frame = cam.frame().unwrap();

            send_result = cam_tx.send(frame.convert());

            frame_counter += 1;
            if frame_counter >= SCAN_INTERVAL {
                scan_tx.send(frame).unwrap();
                frame_counter = 0;
            }
        }
    });

    // SCAN THREAD hands frames to the scanner and passes back the results
    let (result_tx, result_rx) = mpsc::channel();
    let scan_thread = thread::spawn(move || {
        let mut send_result = Ok(());
        while send_result.is_ok() {
            let frame = scan_rx.recv();
            if frame.is_err() { break; }
            let result = scan(&frame.unwrap());
            send_result = result_tx.send(result);
        }
    });

    // meanwhile, main thread draws the camera feed and scan results
    let mut window: PistonWindow =
        WindowSettings::new("QR", [width, height])
        .exit_on_esc(true)
        .build()
        .unwrap();

    let font = Path::new("./assets/Roboto-Regular.ttf");
    let font_ctx = window.create_texture_context();
    let mut glyphs = Glyphs::new(
        &font,
        font_ctx,
        TextureSettings::new()
    ).unwrap();

    let mut cam_ctx = window.create_texture_context();
    let img = cam_rx.recv().unwrap();
    let mut cam_tex = Texture::from_image(
        &mut cam_ctx,
        &img,
        &TextureSettings::new()
    ).unwrap();

    let mut code_ctx = window.create_texture_context();
    let code_dim = width / 2;
    let empty_img_data = vec![0; (code_dim * code_dim * 4) as usize];
    let empty_img = ImageBuffer::from_raw(code_dim, code_dim, empty_img_data).unwrap();
    let mut code_tex = Texture::from_image(
        &mut code_ctx,
        &empty_img,
        &TextureSettings::new()
    ).unwrap();

    let mut scan_result = ScanResult::new();

    while let Some(e) = window.next() {
        if let Ok(img) = cam_rx.try_recv() {
            // filter::binarize_u8_in_place(&mut img);
            cam_tex.update(&mut cam_ctx, &img).unwrap();
        }

        if let Ok(result) = result_rx.try_recv() {
            scan_result = result;
            if let Some(img) = scan_result.code_img {
                code_tex.update(&mut code_ctx, &img).unwrap();
            } else {
                code_tex.update(&mut code_ctx, &empty_img).unwrap();
            }
        }

        window.draw_2d(&e, |c, g, d| {
            piston_window::clear([1.0; 4], g);
            piston_window::image(&cam_tex, c.transform, g);
            for (n, &t) in scan_result.targets.iter().enumerate() {
                let h_line = [t.min.x, t.mid.y, t.max.x, t.mid.y];
                let v_line = [t.mid.x, t.min.y, t.mid.x, t.max.y];
                piston_window::line(LINE_COLOR, 1.0, h_line, c.transform, g);
                piston_window::line(LINE_COLOR, 1.0, v_line, c.transform, g);
                Text::new_color(LINE_COLOR, 12).draw(
                    &n.to_string(),
                    &mut glyphs,
                    &c.draw_state,
                    c.transform.trans(t.min.x, t.min.y),
                    g
                ).unwrap();
            }
    
            if let Some(points) = scan_result.bbox {
                for win in points.windows(2) {
                    let line = [win[0].x, win[0].y, win[1].x, win[1].y];
                    piston_window::line(LINE_COLOR, 1.0, line, c.transform, g);
                }
                let line = [points[2].x, points[2].y, points[0].x, points[0].y];
                piston_window::line(LINE_COLOR, 1.0, line, c.transform, g);

                // let rect = c.viewport.unwrap().rect;
                // let rect = [rect[0] as f64, rect[1] as f64, rect[2] as f64, rect[3] as f64];
                // for pt in points.iter() {
                //     CircleArc::new(LINE_COLOR, 5.0, 0.0, PI*2.0).draw(
                //         [0.0, 0.0, 5.0, 5.0],
                //         &c.draw_state,
                //         c.transform.trans(pt.x, pt.y),
                //         g
                //     );
                // }
            }

            piston_window::image(&code_tex, c.transform, g);

            if let Some(vs) = scan_result.vectors {
                piston_window::line(LINE_COLOR, 1.0, [0.0, 0.0, vs[0].x, vs[0].y], c.transform, g);
                piston_window::line(LINE_COLOR, 1.0, [0.0, 0.0, vs[1].x, vs[1].y], c.transform, g);
            }

            cam_ctx.encoder.flush(d);
            code_ctx.encoder.flush(d);
            glyphs.factory.encoder.flush(d);
        });
    }

    drop(result_rx);
    drop(cam_rx);
    scan_thread.join().unwrap();
    cam_thread.join().unwrap();
}
