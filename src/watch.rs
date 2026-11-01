//! Detecção ao vivo: Raw Input do Windows filtrado pela placa (1189:8890), numa janela só de mensagens.
//! Vê o que a placa manda mesmo com o app sem foco, sem atrapalhar o teclado e o mouse de verdade.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::{offset_of, size_of};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, AtomicU16, Ordering};
use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT};
use windows_sys::Win32::UI::Input::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// O que a placa mandou (só os apertos; os soltares ficam de fora).
pub enum Signal {
    /// mods: mesmos bits de `MODS` (Ctrl 1, Shift 2, Alt 4, Win 8); vk: tecla virtual do Windows.
    Key { mods: u8, vk: u16 },
    Media(u16),
    Mouse { buttons: u8, wheel: i8 },
    Plugged(bool),
}

type Sink = Box<dyn Fn(Signal) + Send>;
static SINK: Mutex<Option<Sink>> = Mutex::new(None);
/// Modificadores que a placa está segurando agora.
static MODS: AtomicU8 = AtomicU8::new(0);
/// Tecla e código de mídia que estão apertados agora: repetição enquanto segura não é aperto novo.
static HELD_KEY: AtomicU16 = AtomicU16::new(0);
static HELD_MEDIA: AtomicU16 = AtomicU16::new(0);

pub fn start(sink: impl Fn(Signal) + Send + 'static) {
    *SINK.lock().unwrap() = Some(Box::new(sink));
    std::thread::spawn(|| unsafe {
        let class = windows_sys::w!("macropad-raw");
        let instance = GetModuleHandleW(std::ptr::null());
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance, lpszClassName: class, ..Default::default() });
        let null = std::ptr::null_mut();
        let hwnd = CreateWindowExW(0, class, std::ptr::null(), 0, 0, 0, 0, 0, HWND_MESSAGE, null, instance, null);
        // Teclado, mouse e controle de mídia, também com o app sem foco; DEVNOTIFY avisa quando a placa entra/sai.
        let devices = [(1, 6), (1, 2), (0x0c, 1)].map(|(page, usage)| RAWINPUTDEVICE {
            usUsagePage: page,
            usUsage: usage,
            dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
            hwndTarget: hwnd,
        });
        RegisterRawInputDevices(devices.as_ptr(), devices.len() as u32, size_of::<RAWINPUTDEVICE>() as u32);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, null, 0, 0) > 0 {
            DispatchMessageW(&msg);
        }
    });
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let signals = match msg {
        WM_INPUT => read(lparam as HRAWINPUT),
        WM_INPUT_DEVICE_CHANGE if is_pad(lparam as HANDLE) => vec![Signal::Plugged(wparam as u32 == GIDC_ARRIVAL)],
        _ => Vec::new(),
    };
    if let Some(sink) = &*SINK.lock().unwrap() {
        signals.into_iter().for_each(sink);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn read(handle: HRAWINPUT) -> Vec<Signal> {
    let header = size_of::<RAWINPUTHEADER>() as u32;
    let mut size = 0;
    unsafe { GetRawInputData(handle, RID_INPUT, std::ptr::null_mut(), &mut size, header) };
    let mut buf = vec![0u64; (size as usize).div_ceil(8)]; // u64: alinhado para ler como RAWINPUT
    let n = unsafe { GetRawInputData(handle, RID_INPUT, buf.as_mut_ptr().cast(), &mut size, header) };
    if n == u32::MAX || (n as usize) < size_of::<RAWINPUTHEADER>() {
        return Vec::new();
    }
    let raw = unsafe { &*buf.as_ptr().cast::<RAWINPUT>() };
    if !is_pad(raw.header.hDevice) {
        return Vec::new();
    }
    let one = match raw.header.dwType {
        RIM_TYPEKEYBOARD => {
            let k = unsafe { raw.data.keyboard };
            let down = k.Flags as u32 & RI_KEY_BREAK == 0;
            let bit = match k.VKey {
                VK_CONTROL => 1,
                VK_SHIFT => 2,
                VK_MENU => 4,
                VK_LWIN | VK_RWIN => 8,
                _ => 0,
            };
            match (bit, down) {
                (0, true) => (HELD_KEY.swap(k.VKey, Ordering::Relaxed) != k.VKey)
                    .then(|| Signal::Key { mods: MODS.load(Ordering::Relaxed), vk: k.VKey }),
                (0, false) => {
                    HELD_KEY.store(0, Ordering::Relaxed);
                    None
                }
                (_, true) => {
                    MODS.fetch_or(bit, Ordering::Relaxed);
                    None
                }
                (_, false) => {
                    MODS.fetch_and(!bit, Ordering::Relaxed);
                    None
                }
            }
        }
        RIM_TYPEMOUSE => {
            let b = unsafe { raw.data.mouse.Anonymous.Anonymous };
            let flags = b.usButtonFlags as u32;
            let pressed = [RI_MOUSE_LEFT_BUTTON_DOWN, RI_MOUSE_RIGHT_BUTTON_DOWN, RI_MOUSE_MIDDLE_BUTTON_DOWN];
            let buttons = pressed.iter().enumerate().filter(|(_, f)| flags & *f != 0).fold(0, |acc, (i, _)| acc | 1 << i);
            let wheel = if flags & RI_MOUSE_WHEEL != 0 { (b.usButtonData as i16).signum() as i8 } else { 0 };
            (buttons != 0 || wheel != 0).then_some(Signal::Mouse { buttons, wheel })
        }
        // Controle de mídia: [report ID, código em 16 bits]; código 0 é o soltar. Girando rápido, o Windows junta
        // vários relatórios numa mensagem só (dwCount), então lê todos. O relatório diz o estado: só conta quando
        // o código muda, porque o mesmo código de novo é a tecla ainda apertada.
        RIM_TYPEHID => {
            let hid = unsafe { raw.data.hid };
            let start = offset_of!(RAWINPUT, data) + offset_of!(RAWHID, bRawData);
            let (len, count) = (hid.dwSizeHid as usize, hid.dwCount as usize);
            let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), n as usize) };
            let Some(reports) = bytes.get(start..start + len * count).filter(|_| len >= 3) else {
                return Vec::new();
            };
            let mut held = HELD_MEDIA.load(Ordering::Relaxed);
            let codes = presses(reports, len, &mut held);
            HELD_MEDIA.store(held, Ordering::Relaxed);
            return codes.into_iter().map(Signal::Media).collect();
        }
        _ => None,
    };
    one.into_iter().collect()
}

/// Apertos novos num lote de relatórios de mídia de `len` bytes; `held` é o código apertado antes do lote e fica com o do fim.
fn presses(reports: &[u8], len: usize, held: &mut u16) -> Vec<u16> {
    reports
        .chunks_exact(len)
        .map(|r| u16::from_le_bytes([r[1], r[2]]))
        .filter(|&code| std::mem::replace(held, code) != code && code != 0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::presses;

    #[test]
    fn conta_cada_aperto_do_lote() {
        let mut held = 0;
        // Knob rápido: dois cliques e os soltares numa mensagem só.
        assert_eq!(presses(&[3, 0xe9, 0, 3, 0, 0, 3, 0xe9, 0, 3, 0, 0], 3, &mut held), [0xe9, 0xe9]);
        // Tecla segurada: o mesmo código repetido é um aperto só, mesmo vindo em outra mensagem.
        assert_eq!(presses(&[3, 0xb5, 0, 3, 0xb5, 0], 3, &mut held), [0xb5]);
        assert_eq!(presses(&[3, 0xb5, 0], 3, &mut held), [] as [u16; 0]);
        assert_eq!(presses(&[3, 0, 0, 3, 0xb5, 0], 3, &mut held), [0xb5]);
    }
}

/// Se o aparelho é a placa. Guarda a resposta: quando ela é desconectada já não dá para perguntar o nome.
fn is_pad(device: HANDLE) -> bool {
    thread_local!(static SEEN: RefCell<HashMap<usize, bool>> = RefCell::default());
    SEEN.with_borrow_mut(|seen| {
        *seen.entry(device as usize).or_insert_with(|| {
            let mut name = [0u16; 512];
            let mut len = name.len() as u32;
            let n = unsafe { GetRawInputDeviceInfoW(device, RIDI_DEVICENAME, name.as_mut_ptr().cast(), &mut len) };
            n != u32::MAX && String::from_utf16_lossy(&name[..n as usize]).to_uppercase().contains("VID_1189&PID_8890")
        })
    })
}
