mod ui;
mod utils;

use bindings::Windows::Win32::Foundation::*;
use bindings::Windows::Win32::Storage::FileSystem::*;
use bindings::Windows::Win32::UI::WindowsAndMessaging::*;
use std::sync::mpsc;
use windows::*;

use bindings::Windows::Win32::System::Pipes::*;
use std::process::exit;
use std::ptr::{null, null_mut};
use std::thread::sleep;
use std::time::Duration;
use std::{mem, thread};
use ui::init;

fn main() {
    init().unwrap();
    // Name Pipe `lyrics_pipe`
    let mut window = ui::LWindow::create_window().unwrap();
    window.show().unwrap();
    unsafe {
        let mut fd = WIN32_FIND_DATAW::default();
        let e_pipe = FindFirstFileW(
            PWSTR(HSTRING::from("\\\\.\\Pipe\\lyrics_pipe").as_wide().as_ptr() as *mut _),
            &mut fd,
        );
        println!("Detecting Any Other Pipe");
        if e_pipe != FindFileHandle(-1) {
            println!("{:?}", e_pipe);
            sleep(Duration::from_millis(1000));
            if FindFirstFileW(
                PWSTR(HSTRING::from("\\\\.\\Pipe\\lyrics_pipe").as_wide().as_ptr() as *mut _),
                &mut fd,
            ) != FindFileHandle(-1)
            {
                println!("Detected Other Pipe");
                exit(0);
            }
        }
    }
    loop {
        let mut h_pipe = unsafe {
            CreateNamedPipeW(
                PWSTR(HSTRING::from("\\\\.\\Pipe\\lyrics_pipe").as_wide().as_ptr() as *mut _),
                0x00000003,
                PIPE_TYPE_MESSAGE.0 | PIPE_READMODE_MESSAGE.0 | 0x00000000,
                255,
                0,
                0,
                NMPWAIT_WAIT_FOREVER.0,
                null_mut(),
            )
        };
        if unsafe { ConnectNamedPipe(h_pipe, null_mut()).as_bool() } {
            println!("Connected");
        }
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            loop {
                unsafe {
                    let mut bytes_read: u32 = 0;
                    let mut buffer: [u8; 1024] = [0; 1024];
                    let read_result = ReadFile(
                        h_pipe,
                        buffer.as_mut_ptr() as *mut _,
                        1024,
                        &mut bytes_read,
                        null_mut(),
                    );
                    // println!("{:?}",read_result.as_bool());
                    if read_result.as_bool() {
                        let message =
                            String::from_utf8_lossy(&buffer[0..bytes_read as usize]).to_string();
                        println!("{}", message);
                        tx.send(message).expect("Send channel error");
                    } else {
                        CloseHandle(h_pipe);
                        h_pipe = CreateNamedPipeW(
                            PWSTR(HSTRING::from("\\\\.\\Pipe\\lyrics_pipe").as_wide().as_ptr()
                                as *mut _),
                            0x00000003,
                            PIPE_TYPE_MESSAGE.0 | PIPE_READMODE_MESSAGE.0 | 0x00000000,
                            255,
                            0,
                            0,
                            NMPWAIT_WAIT_FOREVER.0,
                            null_mut(),
                        );

                        if ConnectNamedPipe(h_pipe, null_mut()).as_bool() {
                            println!("Connected");
                        }
                    }
                    sleep(Duration::from_millis(10));
                }
            }
        });

        unsafe {
            let mut msg = MSG::default();

            loop {
                if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        break;
                    }
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                if let Ok(message) = rx.try_recv() {
                    // println!("MSG: {:?}", message);
                    window.update_lines(message);
                }
                sleep(Duration::from_millis(10));
            }
        }
    }
}
