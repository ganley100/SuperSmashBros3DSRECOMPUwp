//! shows the screens on the TV: a Direct3D 11 swap chain on the app's
//! CoreWindow, drawn into with Direct2D, which scales the console's pictures
//! and writes text without shaders of its own.

use windows::core::{Interface, IUnknown, Result, HSTRING};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Bitmap1, ID2D1DeviceContext, ID2D1Factory1, ID2D1SolidColorBrush, D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
    D2D1_BITMAP_OPTIONS_NONE, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1, D2D1_DEVICE_CONTEXT_OPTIONS_NONE,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_INTERPOLATION_MODE_LINEAR,
    D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1};
use windows::Win32::Graphics::Direct3D11::{D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_MEASURING_MODE_NATURAL,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIDevice, IDXGIFactory2, IDXGISurface, IDXGISwapChain1, DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
    DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT,
};
use windows::UI::Core::CoreWindow;

use crate::layout::{Layout, Rect};

/// what is drawn in a frame.
pub struct Frame<'a> {
    pub top: Option<&'a zakuro_core::Screen>,
    pub bottom: Option<&'a zakuro_core::Screen>,
    pub layout: Layout,
    /// where the touch cursor is on the bottom screen, when it is on, and
    /// whether it touches.
    pub cursor: Option<((f32, f32), bool)>,
    /// a line at the top left: the frame rate, a notice.
    pub status: Option<&'a str>,
    /// a message over everything, why the game can't start say.
    pub message: Option<&'a str>,
    /// draw the pixels sharp instead of smooth.
    pub sharp: bool,
}

/// a picture uploaded for Direct2D, kept from frame to frame.
struct Picture {
    bitmap: ID2D1Bitmap1,
    size: (u32, u32),
}

pub struct Graphics {
    _device: ID3D11Device,
    swap_chain: IDXGISwapChain1,
    context: ID2D1DeviceContext,
    size: (u32, u32),
    text: IDWriteTextFormat,
    small_text: IDWriteTextFormat,
    white: ID2D1SolidColorBrush,
    shade: ID2D1SolidColorBrush,
    cursor: ID2D1SolidColorBrush,
    top: Option<Picture>,
    bottom: Option<Picture>,
}

const BACKGROUND: D2D1_COLOR_F = D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };

impl Graphics {
    /// a swap chain for the window, at the size in pixels.
    pub fn new(window: &CoreWindow, size: (u32, u32)) -> Result<Graphics> {
        unsafe {
            let mut device = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )?;
            let device = device.ok_or_else(windows::core::Error::empty)?;
            let dxgi: IDXGIDevice = device.cast()?;
            let factory: IDXGIFactory2 = dxgi.GetAdapter()?.GetParent()?;
            let description = DXGI_SWAP_CHAIN_DESC1 {
                Width: size.0.max(1),
                Height: size.1.max(1),
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                ..Default::default()
            };
            let window: IUnknown = window.cast()?;
            let swap_chain = factory.CreateSwapChainForCoreWindow(&device, &window, &description, None)?;

            let d2d: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let context = d2d.CreateDevice(&dxgi)?.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;

            let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let format = |size: f32| {
                write.CreateTextFormat(
                    &HSTRING::from("Segoe UI"),
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    &HSTRING::from("en-us"),
                )
            };
            let text = format(30.0)?;
            let small_text = format(22.0)?;
            let brush = |r, g, b, a| context.CreateSolidColorBrush(&D2D1_COLOR_F { r, g, b, a }, None);
            let white = brush(1.0, 1.0, 1.0, 1.0)?;
            let shade = brush(0.0, 0.0, 0.0, 0.7)?;
            let cursor = brush(1.0, 0.85, 0.2, 0.9)?;

            let mut graphics = Graphics {
                _device: device,
                swap_chain,
                context,
                size,
                text,
                small_text,
                white,
                shade,
                cursor,
                top: None,
                bottom: None,
            };
            graphics.target()?;
            Ok(graphics)
        }
    }

    /// points Direct2D at the swap chain's back buffer.
    fn target(&mut self) -> Result<()> {
        unsafe {
            let surface: IDXGISurface = self.swap_chain.GetBuffer(0)?;
            let properties = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                colorContext: std::mem::ManuallyDrop::new(None),
            };
            let bitmap = self.context.CreateBitmapFromDxgiSurface(&surface, Some(&properties))?;
            self.context.SetTarget(&bitmap);
            Ok(())
        }
    }

    /// follows the window to a new size.
    pub fn resize(&mut self, size: (u32, u32)) -> Result<()> {
        if size == self.size || size.0 == 0 || size.1 == 0 {
            return Ok(());
        }
        unsafe {
            // the back buffer can't be resized while Direct2D holds it
            self.context.SetTarget(None);
            self.swap_chain.ResizeBuffers(0, size.0, size.1, DXGI_FORMAT_UNKNOWN, DXGI_SWAP_CHAIN_FLAG(0))?;
        }
        self.size = size;
        self.target()
    }

    pub fn draw(&mut self, frame: &Frame) -> Result<()> {
        let (top_rect, bottom_rect) = frame.layout.place(self.size.0 as f32, self.size.1 as f32);
        if let Some(screen) = frame.top {
            upload(&self.context, &mut self.top, screen)?;
        }
        if let Some(screen) = frame.bottom {
            upload(&self.context, &mut self.bottom, screen)?;
        }
        let mode = if frame.sharp { D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR } else { D2D1_INTERPOLATION_MODE_LINEAR };
        unsafe {
            let context = &self.context;
            context.BeginDraw();
            context.Clear(Some(&BACKGROUND));
            if frame.message.is_none() {
                if let Some(top) = &self.top {
                    context.DrawBitmap(&top.bitmap, Some(&d2d_rect(top_rect)), 1.0, mode, None, None);
                }
                if let (Some(bottom), Some(rect)) = (&self.bottom, bottom_rect) {
                    context.DrawBitmap(&bottom.bitmap, Some(&d2d_rect(rect)), 1.0, mode, None, None);
                    if let Some(((x, y), touching)) = frame.cursor {
                        let point = windows_numerics::Vector2 {
                            X: rect.x + x / 320.0 * rect.width,
                            Y: rect.y + y / 240.0 * rect.height,
                        };
                        let radius = if touching { 7.0 } else { 11.0 };
                        let ellipse = D2D1_ELLIPSE { point, radiusX: radius, radiusY: radius };
                        if touching {
                            context.FillEllipse(&ellipse, &self.cursor);
                        } else {
                            context.DrawEllipse(&ellipse, &self.cursor, 3.0, None);
                        }
                    }
                }
            }
            if let Some(status) = frame.status {
                let area = Rect { x: 24.0, y: 16.0, width: self.size.0 as f32 - 48.0, height: 36.0 };
                context.FillRectangle(&d2d_rect(Rect { width: status.len() as f32 * 11.5 + 16.0, ..area }), &self.shade);
                self.write(status, &self.small_text, Rect { x: area.x + 8.0, ..area });
            }
            if let Some(message) = frame.message {
                // inside the TV's title-safe area, 10% in from each edge
                let (width, height) = (self.size.0 as f32, self.size.1 as f32);
                let area = Rect { x: width * 0.1, y: height * 0.1, width: width * 0.8, height: height * 0.8 };
                self.write(message, &self.text, area);
            }
            match context.EndDraw(None, None) {
                // the GPU was reset: the pictures go and are made again
                Err(error) if error.code() == windows::Win32::Foundation::D2DERR_RECREATE_TARGET => {
                    self.top = None;
                    self.bottom = None;
                    self.target()?;
                }
                other => other?,
            }
            self.swap_chain.Present(1, DXGI_PRESENT(0)).ok()
        }
    }

    unsafe fn write(&self, text: &str, format: &IDWriteTextFormat, area: Rect) {
        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            self.context.DrawText(&wide, format, &d2d_rect(area), &self.white, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL);
        }
    }
}

/// copies an RGBA picture into its bitmap, made again when its size changed.
fn upload(context: &ID2D1DeviceContext, picture: &mut Option<Picture>, screen: &zakuro_core::Screen) -> Result<()> {
    let (pixels, width, height) = (&screen.0, screen.1, screen.2);
    if width == 0 || height == 0 || pixels.len() < (width * height * 4) as usize {
        return Ok(());
    }
    if picture.as_ref().is_none_or(|picture| picture.size != (width, height)) {
        let properties = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_R8G8B8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
            colorContext: std::mem::ManuallyDrop::new(None),
        };
        let bitmap = unsafe { context.CreateBitmap(D2D_SIZE_U { width, height }, None, 0, &properties)? };
        *picture = Some(Picture { bitmap, size: (width, height) });
    }
    let picture = picture.as_ref().expect("made above");
    unsafe { picture.bitmap.CopyFromMemory(None, pixels.as_ptr().cast(), width * 4) }
}

fn d2d_rect(rect: Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: rect.x, top: rect.y, right: rect.right(), bottom: rect.bottom() }
}

