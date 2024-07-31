use std::net::TcpListener;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::spawn;

use windows_capture::frame::ImageFormat;
use windows_capture::{
	capture::GraphicsCaptureApiHandler,
	encoder::ImageEncoder,
	frame::Frame,
	graphics_capture_api::InternalCaptureControl,
	monitor::Monitor,
	settings::{ColorFormat, CursorCaptureSettings, DrawBorderSettings, Settings},
};

lazy_static::lazy_static! {
    pub static ref FRAME: [u8; 1_000_000] = [0; 1_000_000];
	pub static ref FRAME_LEN: AtomicUsize = AtomicUsize::new(0);
}

struct Capture {
	encoder: ImageEncoder
}

impl GraphicsCaptureApiHandler for Capture {
	type Flags = ();
	type Error = Box<dyn std::error::Error + Send + Sync>;

	fn new(_: Self::Flags) -> Result<Self, Self::Error> {
		let encoder = ImageEncoder::new(ImageFormat::Jpeg, ColorFormat::Rgba8);
		Ok(Self{
			encoder
		})
	}

	fn on_frame_arrived(
		&mut self,
		frame: &mut Frame,
		_: InternalCaptureControl,
	) -> Result<(), Self::Error> {
		let mut buffer = frame
			.buffer()
			.unwrap();

		let image = self
			.encoder
			.encode(buffer.as_raw_buffer(), 1024, 768)
			.unwrap();
		FRAME_LEN.store(image.len(), Ordering::Relaxed);

		let mut index = 0;
		unsafe {
			for byte in FRAME.as_slice() {
				if index >= image.len() {
					break;
				}
				let byte = (byte as *const u8 as *mut u8).as_mut().unwrap();
				*byte = image[index];
				index += 1;
			}
		}

		Ok(())
	}
}

fn main() {
	let listener = TcpListener::bind(format!("{}:80", local_ip_address::local_ip().unwrap().to_string())).unwrap();
	println!("Server listening on port 80");

	let monitor = Monitor::from_index(2).unwrap();
	let settings = Settings::new(
        monitor,
        CursorCaptureSettings::Default,
        DrawBorderSettings::Default,
        ColorFormat::Rgba8,
       	(),
    );

	Capture::start_free_threaded(settings).unwrap();
	let mut streams = Vec::new();

	loop {
		let (mut stream, _) = listener.accept().expect("Failed to accept connection");
		let response = format!(
			"HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\n\r\n"
		);
		stream.write_all(response.as_bytes()).unwrap();

		streams.push(spawn(move || {
			loop {
				let image_data = format!(
					"--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
					FRAME_LEN.load(Ordering::Relaxed)
				);

				stream.write_all(image_data.as_bytes()).ok();
				stream.write_all(FRAME.as_slice()).ok();
				stream.write_all(b"\r\n").ok();
				if stream.flush().is_err() {
					break;
				}
	
				let frame_time = std::time::Duration::from_millis(16);
				std::thread::sleep(frame_time);
			}
		}));
	}
}