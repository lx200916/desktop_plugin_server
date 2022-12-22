use crate::utils::*;
use crate::utils::{
    create_animation_manager, create_animation_timer, create_animation_transition_library,
    create_bitmap_from_swap_chain, create_d2d_factory, create_device_context,
    create_dwrite_factory, create_dxgi_device, create_swap_chain, get_desktop_dpi,
};
use anyhow::Result;
use bindings::Windows;
use bindings::Windows::Foundation::Numerics::*;
use bindings::Windows::Win32::Foundation::*;
use bindings::Windows::Win32::Graphics::Direct2D::*;
use bindings::Windows::Win32::Graphics::DirectComposition::*;
use bindings::Windows::Win32::Graphics::DirectWrite::*;
use bindings::Windows::Win32::Graphics::Dxgi::*;
use bindings::Windows::Win32::System::Com::*;
use bindings::Windows::Win32::System::LibraryLoader::*;
use bindings::Windows::Win32::UI::Animation::*;
use bindings::Windows::Win32::UI::WindowsAndMessaging::*;
use once_cell::sync::OnceCell;
use std::ffi::CStr;
use std::fmt::{Debug, Display, Formatter};
use std::os::raw::c_char;
use std::ptr::{null, null_mut};
use std::time::Duration;
use windows::HRESULT;
use windows::*;
use crate::config::Config;

pub fn init() -> Result<()> {
    unsafe {
        SetProcessDPIAware();
        CoInitialize(null_mut())?;
    }
    Ok(())
}

pub struct OSError(pub HRESULT);

impl std::error::Error for OSError {}

impl Display for OSError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.message())
    }
}

impl Debug for OSError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.message())
    }
}

pub struct OSErrorS(pub Error);

impl std::error::Error for OSErrorS {}

impl Display for OSErrorS {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.message())
    }
}

impl Debug for OSErrorS {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.message())
    }
}

const WINDOW_HEIGHT: i32 = 80;
const PADDING_HORIZONTAL: f64 = 10.;
const PADDING_VERTICAL: f64 = 5.;

const DURATION_FADE_IN: Duration = Duration::from_millis(100);
const DURATION_FADE_OUT: Duration = Duration::from_millis(800);
const DURATION_SIZING: Duration = Duration::from_millis(200);
const DURATION_SCROLLING: Duration = Duration::from_millis(350);

pub struct Resources {
    d2d_factory: ID2D1Factory2,
    dc: ID2D1DeviceContext,
    _dcomp_device: IDCompositionDevice,
    _target: IDCompositionTarget,
    swap_chain: IDXGISwapChain1,
    dwrite_factory: IDWriteFactory2,
    brush: ID2D1SolidColorBrush,
    text_format: IDWriteTextFormat1,
    animation_manager: IUIAnimationManager,
    animation_timer: IUIAnimationTimer,
    animation_transition_library: IUIAnimationTransitionLibrary,
    opacity: IUIAnimationVariable,
    bg_width: IUIAnimationVariable,
    bg_height: IUIAnimationVariable,
    line_current_offset: IUIAnimationVariable,
    line_next_offset: IUIAnimationVariable,
    line_next_opacity: IUIAnimationVariable,
}

// #[derive(Debug)]
pub struct LWindow {
    pub(crate) hwnd: HWND,
    resources: OnceCell<Resources>,
    last_line: Option<String>,
    line: Option<String>,
    config: Config,
    movable: bool,
}

fn get_instance() -> HINSTANCE {
    static INSTANCE: OnceCell<HINSTANCE> = OnceCell::new();
    *INSTANCE.get_or_init(|| unsafe { GetModuleHandleW(None) })
}

impl LWindow {
    const CLASS_NAME: &'static str = "LyricsWindow";
    const STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_CLIPSIBLINGS.0 | WS_CLIPCHILDREN.0 | WS_POPUP.0);
    const EX_STYLE: WINDOW_EX_STYLE = WINDOW_EX_STYLE(
        WS_EX_NOREDIRECTIONBITMAP.0
            | WS_EX_LAYERED.0
            | WS_EX_TRANSPARENT.0
            | WS_EX_TOPMOST.0
            | WS_EX_TOOLWINDOW.0,
    );

    fn register_class() -> Result<()> {
        static RESULT: OnceCell<()> = OnceCell::new();
        RESULT
            .get_or_try_init(|| unsafe {
                let instance = get_instance();
                let class_name = HSTRING::from(Self::CLASS_NAME);
                let wc = WNDCLASSW {
                    style: CS_HREDRAW | CS_VREDRAW,
                    hCursor: LoadCursorW(None, IDI_APPLICATION),
                    hInstance: instance,
                    lpfnWndProc: Some(window_proc),
                    lpszClassName: PWSTR(class_name.as_wide().as_ptr() as *mut _),
                    ..Default::default()
                };
                let atom = RegisterClassW(&wc);
                if atom == 0 {
                    Err(OSError(HRESULT::from_thread().into()).into())
                } else {
                    Ok(())
                }
            })
            .map(|()| ())
    }

    pub fn window_proc(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_LBUTTONDOWN=>unsafe{SendMessageW(hwnd, WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), LPARAM(0));LRESULT(0)},
            WM_COPYDATA => unsafe { self.on_update(hwnd, msg, wparam, lparam) },
            WM_DESTROY => self.on_destroy(hwnd, msg, wparam, lparam),
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }
    unsafe fn on_update(
        &mut self,
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let param_ptr = lparam.0 as *const c_char;
        // println!("!{:?}",param_ptr);
        let param = CStr::from_ptr(param_ptr).to_str().unwrap();
        self.update_lines(param.into())
            .expect("TODO: panic message");
        LRESULT(0)
    }
    fn on_destroy(&self, _hwnd: HWND, _msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        unsafe { PostQuitMessage(0) };
        LRESULT(1)
    }
    pub fn create_window(config: Config) -> Result<Self> {
        let (_scale_x, scale_y) = get_scale_factor()?;
        let mut rect = get_workarea_rect()?;
        rect.top = rect.bottom - (WINDOW_HEIGHT as f32 * scale_y).round() as i32;
        let hwnd = unsafe {
            let instance = get_instance();
            Self::register_class()?;
            let class_name = HSTRING::from(Self::CLASS_NAME);
            let window_name = HSTRING::from("LyricsWindow");
            CreateWindowExW(
                Self::EX_STYLE,
                PWSTR(class_name.as_wide().as_ptr() as *mut _),
                PWSTR(window_name.as_wide().as_ptr() as *mut _),
                Self::STYLE,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                None,
                None,
                instance,
                null_mut(),
            )
        };
        if hwnd == HWND(0) {
            return Err(OSError(HRESULT::from_thread().into()).into());
        }
        println!("{:?}", hwnd);
        Ok(Self {
            hwnd,
            resources: OnceCell::new(),
            last_line: None,
            line: None,
            config,movable:false
        })
    }

    pub fn show(&mut self) -> Result<()> {
        unsafe { SetLayeredWindowAttributes(self.hwnd, 0, 255, LWA_ALPHA) };
        unsafe {
            SetWindowLongPtrW(self.get_hwnd(), GWLP_USERDATA, self as *const _ as _);
            ShowWindow(self.get_hwnd(), SW_SHOWNOACTIVATE);
        };
        self.line = Some("!!!".into());
        self.draw()?;
        // self.set_lyrics_timer()?;
        Ok(())
    }
    pub fn change_movable(&mut self){
        let movableStyle = WINDOW_EX_STYLE(
            WS_EX_NOREDIRECTIONBITMAP.0
                | WS_EX_TRANSPARENT.0
                | WS_EX_TOPMOST.0
                | WS_EX_TOOLWINDOW.0,
        );
        let fixedStyle = WINDOW_EX_STYLE(
            WS_EX_NOREDIRECTIONBITMAP.0
                | WS_EX_LAYERED.0
                | WS_EX_TRANSPARENT.0
                | WS_EX_TOPMOST.0
                | WS_EX_TOOLWINDOW.0,
        );
        self.movable=!self.movable;
        if self.movable {

            unsafe { SetWindowLongPtrW(self.get_hwnd(), GWL_EXSTYLE, movableStyle.0 as _) };
        }else {
            unsafe { SetWindowLongPtrW(self.get_hwnd(), GWL_EXSTYLE, fixedStyle.0 as _) };
        }

    }
    pub fn update_lines(&mut self, lyrics: String) -> Result<()> {
        // println!("lyrics： {:?}", lyrics);
        if !lyrics.is_empty() {
            let line_current = lyrics;
            if line_current.starts_with("$$##NMSL") {
                self.change_movable();
                return Ok(());
            }
            // let line_last =self.last_line.as_ref();
            self.last_line = self.line.clone();
            self.schedule_transitions(Some(&line_current))?;
            self.line = Some(line_current);
            // println!("lyrics： {:?}", self.line);
        } else {
            self.last_line = self.line.clone();
            self.line = None;
            self.schedule_transitions(None)?;
        }

        Ok(())
    }

    fn schedule_transitions(&self, line_next: Option<&String>) -> Result<()> {
        println!("{:?}", line_next);
        let Resources {
            dc,
            animation_manager,
            animation_timer,
            animation_transition_library,
            opacity,
            bg_width,
            bg_height,
            line_current_offset,
            line_next_offset,
            line_next_opacity,
            ..
        } = self.get_or_init_resources()?;
        let time_now = unsafe { animation_timer.GetTime() }?;
        let _do_transition = |variable: &IUIAnimationVariable,
                              duration: Duration,
                              initial_value: Option<f64>,
                              final_value: f64,
                              skip_if_hidden: bool,
                              acceleration_ratio: f64,
                              deceleration_ratio: f64|
                              -> Result<()> {
            if initial_value.is_none() && unsafe { variable.GetFinalValue() }? == final_value {
                return Ok(());
            }
            let duration = duration.as_secs_f64();
            unsafe {
                let transition = animation_transition_library
                    .CreateAccelerateDecelerateTransition(
                        duration,
                        final_value,
                        acceleration_ratio,
                        deceleration_ratio,
                    )?;
                if let Some(initial_value) = initial_value {
                    transition.SetInitialValue(initial_value)?;
                }
                if skip_if_hidden && opacity.GetValue()? == 0. {
                    transition.SetInitialValue(final_value)?;
                }
                animation_manager.ScheduleTransition(variable, &transition, time_now)?;
            }
            Ok(())
        };
        let do_transition_ease_out = |variable: &IUIAnimationVariable,
                                      duration: Duration,
                                      initial_value: Option<f64>,
                                      final_value: f64,
                                      skip_if_hidden: bool|
                                      -> Result<()> {
            _do_transition(
                variable,
                duration,
                initial_value,
                final_value,
                skip_if_hidden,
                0.,
                1.,
            )
        };
        let do_transition_linear = |variable: &IUIAnimationVariable,
                                    duration: Duration,
                                    initial_value: Option<f64>,
                                    final_value: f64,
                                    skip_if_hidden: bool|
                                    -> Result<()> {
            _do_transition(
                variable,
                duration,
                initial_value,
                final_value,
                skip_if_hidden,
                0.,
                0.,
            )
        };
        match line_next {
            Some(line_next) if !line_next.is_empty() => {
                let size = unsafe { dc.GetSize() };
                do_transition_ease_out(opacity, DURATION_FADE_IN, None, 1., false)?;

                let metrics = self.get_text_metrics(line_next, size.width, size.height)?;
                let final_bg_width = metrics.width as f64 + 2. * PADDING_HORIZONTAL;
                do_transition_ease_out(bg_width, DURATION_SIZING, None, final_bg_width, true)?;

                let final_bg_height = metrics.height as f64 + 2. * PADDING_VERTICAL;
                do_transition_ease_out(bg_height, DURATION_SIZING, None, final_bg_height, true)?;

                let vertical_offset = size.height as f64 / 3.;
                do_transition_ease_out(
                    line_current_offset,
                    DURATION_SCROLLING,
                    Some(0.),
                    -vertical_offset,
                    true,
                )?;
                do_transition_ease_out(
                    line_next_offset,
                    DURATION_SCROLLING,
                    Some(vertical_offset),
                    0.,
                    true,
                )?;

                do_transition_ease_out(line_next_opacity, DURATION_SCROLLING, Some(0.), 1., true)?;
            }
            _ => {
                do_transition_linear(opacity, DURATION_FADE_OUT, None, 0., false)?;
            }
        }
        Ok(())
    }

    fn get_or_init_resources(&self) -> Result<&Resources> {
        self.resources.get_or_try_init(|| {
            let (dpi_x, dpi_y) = get_desktop_dpi()?;
            let d2d_factory = create_d2d_factory()?;
            let dxgi_device = create_dxgi_device()?;
            let _dcomp_device: IDCompositionDevice =
                unsafe { DCompositionCreateDevice(&dxgi_device) }?;
            let visual = unsafe { _dcomp_device.CreateVisual() }?;
            let dc = create_device_context(&dxgi_device, &d2d_factory, dpi_x, dpi_y)?;
            let swap_chain = create_swap_chain(self.hwnd, &dxgi_device)?;
            create_bitmap_from_swap_chain(&dc, &swap_chain, dpi_x, dpi_y)?;
            unsafe { visual.SetContent(&swap_chain) }?;
            let _target = unsafe { _dcomp_device.CreateTargetForHwnd(self.hwnd, BOOL(1)) }?;
            unsafe {
                _target.SetRoot(&visual)?;
                _dcomp_device.Commit()?;
            }
            let brush = unsafe { dc.CreateSolidColorBrush(&D2D1_COLOR_F::default(), null()) }?;
            let dwrite_factory = create_dwrite_factory()?;
            let text_format: IDWriteTextFormat1 = unsafe {
                dwrite_factory
                    .CreateTextFormat(
                        "Segoe UI",
                        None,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_FONT_STYLE_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        self.config.font_size as f32,
                        "",
                    )?
                    .cast()
            }?;
            {
                unsafe {
                    text_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
                    text_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                }
                let font_fallback_builder = unsafe { dwrite_factory.CreateFontFallbackBuilder() }?;
                let ranges = DWRITE_UNICODE_RANGE {
                    first: 0x0,
                    last: 0xffffffff,
                };
                let fallback_family_names = [
                    HSTRING::from("Segoe UI Emoji"),
                    HSTRING::from("Segoe UI Symbol"),
                    HSTRING::from("Helvetica"),
                    HSTRING::from("Microsoft YaHei UI"),
                ];
                let fallback_family_names = fallback_family_names
                    .iter()
                    .map(|name| name.as_wide().as_ptr())
                    .collect::<Vec<*const u16>>();
                unsafe {
                    font_fallback_builder.AddMapping(
                        &ranges,
                        1,
                        fallback_family_names.as_ptr(),
                        fallback_family_names.len() as u32,
                        None,
                        None,
                        None,
                        1.,
                    )?;
                    font_fallback_builder.AddMappings(dwrite_factory.GetSystemFontFallback()?)?;
                    let font_fallback = font_fallback_builder.CreateFontFallback()?;
                    text_format.SetFontFallback(font_fallback)?;
                }
            }
            let animation_manager = create_animation_manager()?;
            let animation_timer = create_animation_timer()?;
            let animation_timer_handler: IUIAnimationTimerEventHandler =
                UIAnimationTimerEventHandler::new(self.hwnd, &self).into();
            unsafe { animation_timer.SetTimerEventHandler(animation_timer_handler) }?;
            let timer_update_handler: IUIAnimationTimerUpdateHandler = animation_manager.cast()?;
            unsafe {
                animation_timer.SetTimerUpdateHandler(
                    &timer_update_handler,
                    UI_ANIMATION_IDLE_BEHAVIOR_DISABLE,
                )
            }?;
            let animation_transition_library = create_animation_transition_library()?;
            let opacity = unsafe { animation_manager.CreateAnimationVariable(0.) }?;
            let bg_width = unsafe { animation_manager.CreateAnimationVariable(0.) }?;
            let bg_height = unsafe { animation_manager.CreateAnimationVariable(0.) }?;
            let line_current_offset = unsafe { animation_manager.CreateAnimationVariable(0.) }?;
            let line_next_offset = unsafe { animation_manager.CreateAnimationVariable(0.) }?;
            let line_next_opacity = unsafe { animation_manager.CreateAnimationVariable(0.) }?;
            Ok(Resources {
                d2d_factory,
                dc,
                _dcomp_device,
                _target,
                swap_chain,
                brush,
                dwrite_factory,
                text_format,
                animation_manager,
                animation_timer,
                animation_transition_library,
                opacity,
                bg_width,
                bg_height,
                line_current_offset,
                line_next_offset,
                line_next_opacity,
            })
        })
    }

    fn draw(&self) -> Result<()> {
        let Resources {
            d2d_factory,
            dc,
            swap_chain,
            brush,
            opacity,
            bg_width,
            bg_height,
            line_current_offset,
            line_next_offset,
            line_next_opacity,
            ..
        } = self.get_or_init_resources()?;
        let Self {
            line, last_line, ..
        } = self;
        // println!("line {:?}", line);
        unsafe {
            dc.BeginDraw();
            dc.Clear(null_mut());
            let opacity = opacity.GetValue()? as f32;
            dc.PushLayer(
                &D2D1_LAYER_PARAMETERS {
                    contentBounds: D2D_RECT_F {
                        left: -f32::INFINITY,
                        top: -f32::INFINITY,
                        right: f32::INFINITY,
                        bottom: f32::INFINITY,
                    },
                    geometricMask: None,
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2::identity(),
                    opacity,
                    opacityBrush: None,
                    layerOptions: D2D1_LAYER_OPTIONS_NONE,
                },
                None,
            );
            brush.SetColor(&D2D1_COLOR_F {
                r: self.config.bg_color[0],
                g: self.config.bg_color[1],
                b: self.config.bg_color[2],
                a: self.config.bg_color[3],
            });
            let bg_width = bg_width.GetValue()? as f32;
            let bg_height = bg_height.GetValue()? as f32;
            let size = dc.GetSize();
            let left = (size.width - bg_width) / 2.;
            let top = (size.height - bg_height) / 2.;
            let right = left + bg_width;
            let bottom = top + bg_height;
            let bg_rounded_rect = &D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left,
                    top,
                    right,
                    bottom,
                },
                radiusX: 4.,
                radiusY: 4.,
            };
            let bg_geometry = d2d_factory.CreateRoundedRectangleGeometry(bg_rounded_rect)?;
            dc.FillGeometry(&bg_geometry, brush, None);
            let line_next_opacity = line_next_opacity.GetValue()? as f32;
            let line_current_opacity = 1. - line_next_opacity;
            dc.PushLayer(
                &D2D1_LAYER_PARAMETERS {
                    contentBounds: D2D_RECT_F {
                        left: -f32::INFINITY,
                        top: -f32::INFINITY,
                        right: f32::INFINITY,
                        bottom: f32::INFINITY,
                    },
                    geometricMask: Some((&bg_geometry).into()),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2::identity(),
                    opacity: line_current_opacity,
                    opacityBrush: None,
                    layerOptions: D2D1_LAYER_OPTIONS_NONE,
                },
                None,
            );
            let line_current_offset = line_current_offset.GetValue()? as f32;
            let line_next_offset = line_next_offset.GetValue()? as f32;
            if let Some(line_current) = last_line {
                self.draw_text(
                    line_current,
                    &D2D_RECT_F {
                        left: 0.,
                        top: line_current_offset,
                        right: size.width,
                        bottom: line_current_offset + size.height,
                    },
                )?;
            }
            dc.PopLayer();
            dc.PushLayer(
                &D2D1_LAYER_PARAMETERS {
                    contentBounds: D2D_RECT_F {
                        left: -f32::INFINITY,
                        top: -f32::INFINITY,
                        right: f32::INFINITY,
                        bottom: f32::INFINITY,
                    },
                    geometricMask: Some((&bg_geometry).into()),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2::identity(),
                    opacity: line_next_opacity,
                    opacityBrush: None,
                    layerOptions: D2D1_LAYER_OPTIONS_NONE,
                },
                None,
            );
            if let Some(line_next) = line {
                self.draw_text(
                    line_next,
                    &D2D_RECT_F {
                        left: 0.,
                        top: line_next_offset,
                        right: size.width,
                        bottom: line_next_offset + size.height,
                    },
                )?;
            }
            dc.PopLayer();
            dc.PopLayer();
            dc.EndDraw(null_mut(), null_mut())?;
            swap_chain.Present(0, 0)?;
        }
        Ok(())
    }
    fn get_hwnd(&self) -> HWND {
        self.hwnd
    }
    fn create_text_layout(
        &self,
        text: &str,
        max_width: f32,
        max_height: f32,
    ) -> Result<IDWriteTextLayout> {
        let Resources {
            dwrite_factory,
            text_format,
            ..
        } = self.get_or_init_resources()?;
        let string = HSTRING::from(text);
        unsafe {
            dwrite_factory
                .CreateTextLayout(
                    PWSTR(string.as_wide().as_ptr() as *mut _),
                    string.len() as u32,
                    text_format,
                    max_width,
                    max_height,
                )
                .map_err(|e| e.into())
        }
    }

    fn draw_text(&self, text: &str, rect: &D2D_RECT_F) -> Result<()> {
        let Resources { dc, brush, .. } = self.get_or_init_resources()?;
        let text_layout =
            self.create_text_layout(text, rect.right - rect.left, rect.bottom - rect.top)?;


        unsafe {
            brush.SetColor(&D2D1_COLOR_F {
                r: self.config.text_color[0],
                g: self.config.text_color[1],
                b: self.config.text_color[2],
                a: self.config.text_color[3],
            });
            dc.DrawTextLayout(
                &D2D_POINT_2F {
                    x: rect.left,
                    y: rect.top,
                },
                &text_layout,
                brush,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
            );
        }
        Ok(())
    }

    fn get_text_metrics(
        &self,
        text: &str,
        max_width: f32,
        max_height: f32,
    ) -> Result<DWRITE_TEXT_METRICS> {
        let text_layout = self.create_text_layout(text, max_width, max_height)?;
        unsafe { text_layout.GetMetrics() }.map_err(|e| e.into())
    }
}

pub unsafe fn get_window_instance<'a>(hwnd: HWND) -> Option<&'a mut LWindow> {
    let window_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    if window_ptr == 0 {
        None
    } else {
        Some(&mut *(window_ptr as *mut LWindow))
    }
}

#[implement(Windows::Win32::UI::Animation::IUIAnimationTimerEventHandler)]
struct UIAnimationTimerEventHandler {
    hwnd: HWND,
    window_ptr: *const LWindow,
}

#[allow(non_snake_case)]
impl UIAnimationTimerEventHandler {
    fn new(hwnd: HWND, window_ptr: &LWindow) -> Self {
        Self {
            hwnd,
            window_ptr: window_ptr as *const LWindow,
        }
    }

    fn OnPreUpdate(&self) -> HRESULT {
        S_OK
    }

    fn OnPostUpdate(&self) -> HRESULT {
        let window: &LWindow = unsafe { &*self.window_ptr };
        window.draw().unwrap();
        S_OK
    }

    fn OnRenderingTooSlow(&self, _fps: u32) -> HRESULT {
        S_OK
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let window: Option<&mut LWindow> = get_window_instance(hwnd);
    // println!("LWindow {:?}", window.is_some());
    match window {
        Some(window) => window.window_proc(hwnd, msg, wparam, lparam),
        None => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
