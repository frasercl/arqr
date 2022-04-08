
use std::{thread, sync::mpsc};
use image::buffer::ConvertBuffer;
use nokhwa::Camera;
use piston_window::{
    PistonWindow,
    Texture,
    TextureSettings,
    WindowSettings,
};
use arqr::{
    target::{find_position_targets, GridLine}, 
    bitmap::Bitmap, 
    filter,
};

const FPS: u32 = 15;
const SCAN_INTERVAL: u32 = 3;
const LINE_COLOR: [f32; 4] = [0.0, 1.0, 0.0, 1.0]; // green

fn gridline_to_piston_line(line: GridLine) -> [f64; 4] {
    let x = line.x as f64;
    let y = line.y as f64;
    let p2 = line.p2 as f64;
    if line.vert {
        [x, y, x, p2]
    } else {
        [x, y, p2, y]
    }
}

fn main() {
    let mut cam = Camera::new(0, None).unwrap();
    let res = cam.resolution();
    
    // CAM THREAD gets frames from the camera and hands it to the scanner and drawer
    let (cam_sender, cam_recver) = mpsc::channel();
    let (scan_sender, scan_recver) = mpsc::channel();
    let cam_thread = thread::spawn(move || {
        cam.set_frame_rate(FPS).unwrap();
        cam.open_stream().unwrap();
        let mut frame_counter = 0;

        let mut send_result = Ok(());
        while send_result.is_ok() {
            let frame = cam.frame().unwrap();
            
            //filter::binarize_u8_in_place(&mut frame);
            send_result = cam_sender.send(frame.convert());

            frame_counter += 1;
            if frame_counter >= SCAN_INTERVAL {
                scan_sender.send(frame).unwrap();
                frame_counter = 0;
            }
        }
    });

    // SCAN THREAD hands frames to the scanner and passes back the results
    let (result_sender, result_recver) = mpsc::channel();
    let scan_thread = thread::spawn(move || {
        let mut send_result = Ok(());
        while send_result.is_ok() {
            let frame = scan_recver.recv();
            if frame.is_err() { break; }
            let bmp = Bitmap::from_u8_img_dynamic(&frame.unwrap());
            let targets = find_position_targets(&bmp);
            send_result = result_sender.send(targets);
        }
    });

    // meanwhile, this thread draws the camera feed and scan results
    let mut window: PistonWindow = WindowSettings::new("QR", [res.width(), res.height()])
        .exit_on_esc(true)
        .build()
        .unwrap();
    let mut ctx = window.create_texture_context();
    let img = cam_recver.recv().unwrap();
    let mut tex = Texture::from_image(&mut ctx, &img, &TextureSettings::new()).unwrap();
    let mut current_targets = Vec::new();

    while let Some(e) = window.next() {
        if let Ok(img) = cam_recver.try_recv() {
            // filter::edge_2_in_place(&mut img);
            tex.update(&mut ctx, &img).unwrap();
        }

        if let Ok(targets) = result_recver.try_recv() {
            // let num_found = targets.iter().count();
            // if num_found > 0 {
            //     println!("Found {} potential targets", num_found);
            // }
            current_targets = targets;
        }

        window.draw_2d(&e, |c, g, d| {
            piston_window::clear([1.0; 4], g);
            piston_window::image(&tex, c.transform, g);
            for &g_line in current_targets.iter() {
                let p_line = gridline_to_piston_line(g_line);
                piston_window::line(LINE_COLOR, 1.0, p_line, c.transform, g);
            }
            ctx.encoder.flush(d);
        });
    }

    drop(result_recver);
    drop(cam_recver);
    scan_thread.join().unwrap();
    cam_thread.join().unwrap();
}
