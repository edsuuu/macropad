#![windows_subsystem = "windows"]
//! Configura a macropad chinesa de 3 teclas + knob (CH57x, USB 1189:8890).
//! Protocolo do modelo 8890 do ch57x-keyboard-tool (kriomant); a placa guarda tudo na própria memória.
//! A tela é uma página (web/index.html, com three.js) numa janela WebView2; aqui fica o resto.

mod watch;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::os::windows::process::CommandExt;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::windows::{IconExtWindows, WindowBuilderExtWindows};
use tao::window::{Icon, WindowBuilder};
use watch::Signal;

const LAYER: u8 = 1; // ponytail: só a camada 1; a placa tem 3, mas a 3x1 não tem como trocar de camada
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// (id da tecla no protocolo, nome na tela), da esquerda para a direita com o knob à esquerda:
/// na placa o id 1 é a tecla da direita. O knob n usa 13 + 3n + (esquerda, apertar, direita).
const SLOTS: [(u8, &str); 6] = [
    // Com o knob à esquerda a placa fica "de cabeça para baixo" (no normal do protocolo o knob é à direita): a tecla da esquerda é a id 3.
    (3, "Tecla 1"),
    (2, "Tecla 2"),
    (1, "Tecla 3"),
    (13, "Knob: girar à esquerda"),
    (14, "Knob: apertar"),
    (15, "Knob: girar à direita"),
];

/// Bits do byte de modificadores do HID.
const MODS: [(u8, &str); 4] = [(1, "Ctrl"), (2, "Shift"), (4, "Alt"), (8, "Win")];

/// Códigos da página HID "Consumer".
const MEDIA: [(u16, &str); 11] = [
    (0xe9, "Volume +"),
    (0xea, "Volume -"),
    (0xe2, "Mudo"),
    (0xcd, "Play / Pause"),
    (0xb5, "Próxima faixa"),
    (0xb6, "Faixa anterior"),
    (0xb7, "Parar"),
    (0x192, "Calculadora"),
    (0x223, "Navegador: início"),
    (0x224, "Navegador: voltar"),
    (0x225, "Navegador: avançar"),
];

/// (botões, roda): botões 1 esquerdo, 2 direito, 4 meio; roda +1 sobe, -1 desce.
const MOUSE: [((u8, i8), &str); 5] = [
    ((1, 0), "Clique esquerdo"),
    ((2, 0), "Clique direito"),
    ((4, 0), "Clique do meio"),
    ((0, 1), "Rolar para cima"),
    ((0, -1), "Rolar para baixo"),
];

#[derive(Clone, PartialEq, Serialize, Deserialize)]
enum Action {
    /// mods: bits de `MODS`; code: código HID da tecla (0 = só os modificadores).
    Key { mods: u8, code: u8 },
    Media(u16),
    Mouse { buttons: u8, wheel: i8 },
    /// Linha de comando que o escutador roda quando a placa manda o atalho desta posição.
    Command(String),
}

const DEFAULT: [Action; 6] = [
    Action::Media(0xb6),
    Action::Media(0xcd),
    Action::Media(0xb5),
    Action::Media(0xea),
    Action::Media(0xe2),
    Action::Media(0xe9),
];

/// A página e o three.js, embutidos no exe.
const FILES: [(&str, &str, &[u8]); 3] = [
    ("/", "text/html; charset=utf-8", include_bytes!("../web/index.html")),
    ("/three.module.min.js", "text/javascript", include_bytes!("../web/three.module.min.js")),
    ("/RoomEnvironment.js", "text/javascript", include_bytes!("../web/RoomEnvironment.js")),
];

/// Nomes e códigos HID das teclas que dá para escolher.
fn key_names() -> Vec<(String, u8)> {
    let mut k = vec![("(nenhuma)".to_string(), 0)];
    k.extend((b'A'..=b'Z').map(|c| ((c as char).to_string(), c - b'A' + 0x04)));
    k.extend((1..=9).chain([0]).zip(0x1e..).map(|(n, c)| (n.to_string(), c)));
    k.extend((1..=12).map(|n| (format!("F{n}"), 0x39 + n)));
    k.extend((13..=24).map(|n| (format!("F{n}"), 0x5b + n)));
    k.extend(
        [
            ("Enter", 0x28), ("Esc", 0x29), ("Backspace", 0x2a), ("Tab", 0x2b), ("Espaço", 0x2c),
            ("-", 0x2d), ("=", 0x2e), ("[", 0x2f), ("]", 0x30), ("\\", 0x31), (";", 0x33), ("'", 0x34),
            ("`", 0x35), (",", 0x36), (".", 0x37), ("/", 0x38), ("Caps Lock", 0x39), ("Print Screen", 0x46),
            ("Scroll Lock", 0x47), ("Pause", 0x48), ("Insert", 0x49), ("Home", 0x4a), ("Page Up", 0x4b),
            ("Delete", 0x4c), ("End", 0x4d), ("Page Down", 0x4e), ("Seta direita", 0x4f),
            ("Seta esquerda", 0x50), ("Seta baixo", 0x51), ("Seta cima", 0x52), ("Num Lock", 0x53),
            ("Num /", 0x54), ("Num *", 0x55), ("Num -", 0x56), ("Num +", 0x57), ("Num Enter", 0x58),
            ("Num .", 0x63), ("Menu", 0x65),
        ]
        .map(|(n, c)| (n.to_string(), c)),
    );
    k.extend((1..=9).chain([0]).zip(0x59..).map(|(n, c)| (format!("Num {n}"), c)));
    k
}

/// A tecla virtual do Windows que um código HID vira.
/// ponytail: as de pontuação seguem o layout americano; no ABNT2 elas podem não ser reconhecidas ao vivo
fn hid_to_vk(code: u8) -> Option<u16> {
    let c = code as u16;
    Some(match code {
        0x04..=0x1d => 0x41 + c - 0x04, // A-Z
        0x1e..=0x26 => 0x31 + c - 0x1e, // 1-9
        0x27 => 0x30,
        0x3a..=0x45 => 0x70 + c - 0x3a, // F1-F12
        0x68..=0x73 => 0x7c + c - 0x68, // F13-F24
        0x59..=0x61 => 0x61 + c - 0x59, // Num 1-9
        0x62 => 0x60,
        _ => {
            const OTHERS: [(u8, u16); 38] = [
                (0x28, 0x0d), (0x29, 0x1b), (0x2a, 0x08), (0x2b, 0x09), (0x2c, 0x20), (0x2d, 0xbd), (0x2e, 0xbb),
                (0x2f, 0xdb), (0x30, 0xdd), (0x31, 0xdc), (0x33, 0xba), (0x34, 0xde), (0x35, 0xc0), (0x36, 0xbc),
                (0x37, 0xbe), (0x38, 0xbf), (0x39, 0x14), (0x46, 0x2c), (0x47, 0x91), (0x48, 0x13), (0x49, 0x2d),
                (0x4a, 0x24), (0x4b, 0x21), (0x4c, 0x2e), (0x4d, 0x23), (0x4e, 0x22), (0x4f, 0x27), (0x50, 0x25),
                (0x51, 0x28), (0x52, 0x26), (0x53, 0x90), (0x54, 0x6f), (0x55, 0x6a), (0x56, 0x6d), (0x57, 0x6b),
                (0x58, 0x0d), (0x63, 0x6e), (0x65, 0x5d),
            ];
            return OTHERS.iter().find(|o| o.0 == code).map(|o| o.1);
        }
    })
}

fn msg(b: &[u8]) -> [u8; 64] {
    let mut m = [0; 64];
    m[..b.len()].copy_from_slice(b);
    m
}

/// LED da placa, como o `led <modo>` da ferramenta original. O byte é `cor << 4 | efeito`, como no modelo 884x:
/// efeito 0 apaga, 1 aceso, 2..4 acendem ao apertar; cor 1..7 (vermelho a roxo), 0 = as cores da placa.
/// Testado nesta placa: 0x62 (azul, acende ao apertar e apaga).
fn led_messages(mode: u8) -> [[u8; 64]; 3] {
    [msg(&[0x03, 0xa1, 0x01]), msg(&[0x03, 0xb0, 0x18, mode]), msg(&[0x03, 0xaa, 0xa1])]
}

/// Mensagens de 64 bytes que gravam `a` na posição `slot`, iguais às da ferramenta original.
fn messages(slot: usize, a: &Action) -> Vec<[u8; 64]> {
    let (id, l) = (SLOTS[slot].0, LAYER << 4);
    // A placa espera uma tecla vazia antes da sequência (aqui, de uma tecla só).
    let key = |mods, code| [msg(&[0x03, id, l | 1, 1, 0, 0, 0]), msg(&[0x03, id, l | 1, 1, 1, mods, code])];
    let mut out = vec![msg(&[0x03, 0xfe, LAYER, 1, 1])];
    match *a {
        Action::Key { mods, code } => out.extend(key(mods, code)),
        // ponytail: Ctrl+Shift+Alt + F13..F18 fixos; se algum app já usar, trocar a base aqui, no `listen` e no `hits`
        Action::Command(_) => out.extend(key(0b111, 0x68 + slot as u8)),
        Action::Media(code) => {
            let [lo, hi] = code.to_le_bytes();
            out.push(msg(&[0x03, id, l | 2, lo, hi]));
        }
        Action::Mouse { buttons, wheel } => out.push(msg(&[0x03, id, l | 3, buttons, 0, 0, wheel as u8])),
    }
    out.push(msg(&[0x03, 0xaa, 0xaa]));
    out
}

/// A interface de configuração da placa (página HID do fabricante, 0xFF00).
fn open() -> Result<hidapi::HidDevice, String> {
    let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
    let info = api
        .device_list()
        .find(|d| d.vendor_id() == 0x1189 && d.product_id() == 0x8890 && d.usage_page() == 0xff00)
        .ok_or("placa não encontrada, confira se ela está na USB")?;
    info.open_device(&api).map_err(|e| e.to_string())
}

fn send(msgs: impl IntoIterator<Item = [u8; 64]>) -> Result<(), String> {
    let dev = open()?;
    for m in msgs {
        let mut report = [0; 65]; // relatório 3 de 64 bytes: o 0x03 que abre a mensagem é o report ID
        report[..64].copy_from_slice(&m);
        dev.write(&report).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write(slots: &[Action; 6]) -> Result<(), String> {
    send(slots.iter().enumerate().flat_map(|(slot, a)| messages(slot, a)))
}

fn set_led(mode: u8) -> String {
    match send(led_messages(mode)) {
        Ok(()) => format!("LED da placa gravado ({mode:#04x})."),
        Err(e) => format!("Não gravou o LED: {e}."),
    }
}

fn config_path() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("Macropad").join("config.json")
}

fn load() -> [Action; 6] {
    std::fs::read_to_string(config_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(DEFAULT)
}

fn save(slots: &[Action; 6]) -> std::io::Result<()> {
    let path = config_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, serde_json::to_string_pretty(slots).unwrap())
}

/// Grava na placa, guarda no PC e acerta o escutador; devolve a frase para a tela.
fn gravar(slots: &[Action; 6]) -> String {
    match write(slots) {
        Ok(()) => match save(slots).and_then(|()| sync_listener(slots)) {
            Ok(()) => "Gravado na placa.".into(),
            Err(e) => format!("Gravado na placa, mas: {e}"),
        },
        Err(e) => format!("Não gravou: {e}."),
    }
}

/// Roda a linha de comando como o "Executar" do Windows: programa, arquivo, pasta ou site.
fn run(cmd: &str) {
    if !cmd.trim().is_empty() {
        let _ = std::process::Command::new("cmd").raw_arg(format!("/C start \"\" {cmd}")).creation_flags(CREATE_NO_WINDOW).spawn();
    }
}

/// `macropad --escutar`: fica em segundo plano esperando os atalhos das posições com "Comando".
fn listen() {
    use windows_sys::Win32::UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*};
    for slot in 0..6 {
        let vk = VK_F13 as u32 + slot as u32;
        // Falha quando outro escutador já pegou os atalhos: fica aquele.
        if unsafe { RegisterHotKey(std::ptr::null_mut(), slot, MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT, vk) } == 0 {
            return;
        }
    }
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        // Relê a configuração a cada toque: o app pode ter gravado outra.
        if msg.message == WM_HOTKEY && let Action::Command(cmd) = &load()[msg.wParam] {
            run(cmd);
        }
    }
}

/// Com algum "Comando", deixa o escutador rodando agora e junto com o Windows; sem nenhum, tira do início.
fn sync_listener(slots: &[Action; 6]) -> std::io::Result<()> {
    const RUN: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let exe = std::env::current_exe()?;
    let mut reg = std::process::Command::new("reg");
    if slots.iter().any(|a| matches!(a, Action::Command(_))) {
        std::process::Command::new(&exe).arg("--escutar").spawn()?;
        reg.args(["add", RUN, "/v", "Macropad", "/t", "REG_SZ", "/f", "/d"]).arg(format!("\"{}\" --escutar", exe.display()));
    } else {
        reg.args(["delete", RUN, "/v", "Macropad", "/f"]);
    }
    reg.creation_flags(CREATE_NO_WINDOW).status().map(drop)
}

/// As posições cuja ação gravada é o que a placa acabou de mandar.
fn hits(signal: &Signal, slots: &[Action; 6]) -> Vec<usize> {
    (0..6)
        .filter(|&i| match (signal, &slots[i]) {
            (Signal::Key { mods, vk }, Action::Key { mods: m, code }) => mods == m && hid_to_vk(*code) == Some(*vk),
            (Signal::Key { mods, vk }, Action::Command(_)) => *mods == 0b111 && *vk == 0x7c + i as u16,
            (Signal::Media(c), Action::Media(m)) => c == m,
            (Signal::Mouse { buttons, wheel }, Action::Mouse { buttons: b, wheel: w }) => buttons == b && wheel == w,
            _ => false,
        })
        .collect()
}

/// O que a placa mandou, em texto.
fn describe(signal: &Signal, keys: &[(String, u8)]) -> String {
    match *signal {
        Signal::Key { mods, vk } => {
            let mut parts: Vec<String> = MODS.iter().filter(|m| mods & m.0 != 0).map(|m| m.1.to_string()).collect();
            parts.push(keys.iter().find(|k| hid_to_vk(k.1) == Some(vk)).map_or(format!("tecla {vk:#04x}"), |k| k.0.clone()));
            parts.join("+")
        }
        Signal::Media(c) => MEDIA.iter().find(|m| m.0 == c).map_or(format!("mídia {c:#06x}"), |m| m.1.into()),
        Signal::Mouse { buttons, wheel } => MOUSE.iter().find(|m| m.0 == (buttons, wheel)).map_or("mouse".into(), |m| m.1.into()),
        Signal::Plugged(on) => if on { "conectada" } else { "desconectada" }.into(),
    }
}

enum UserEvent {
    Page(String),
    Pad(Signal),
}

fn main() -> wry::Result<()> {
    if std::env::args().any(|a| a == "--escutar") {
        listen();
        return Ok(());
    }
    // `macropad --gravar`: grava de novo na placa a configuração guardada, sem abrir a janela.
    if std::env::args().any(|a| a == "--gravar") {
        println!("{}", gravar(&load()));
        return Ok(());
    }
    // `macropad --led <modo>`: só o modo do LED da placa.
    if let Some(mode) = std::env::args().skip_while(|a| a != "--led").nth(1).and_then(|m| m.parse().ok()) {
        println!("{}", set_led(mode));
        return Ok(());
    }
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title("Macropad")
        // O ícone que o build.rs pôs no .exe (assets/icon.rc, id 1).
        .with_window_icon(Icon::from_resource(1, None).ok())
        .with_taskbar_icon(Icon::from_resource(1, None).ok())
        .with_inner_size(LogicalSize::new(1200.0, 760.0))
        .with_min_inner_size(LogicalSize::new(900.0, 560.0))
        .build(&event_loop)
        .expect("criar a janela");
    let page = event_loop.create_proxy();
    let mut context = wry::WebContext::new(Some(config_path().with_file_name("webview")));
    let webview = wry::WebViewBuilder::new_with_web_context(&mut context)
        .with_custom_protocol("macropad".into(), |_, request| {
            let file = FILES.iter().find(|f| f.0 == request.uri().path());
            let (mime, body) = file.map_or(("text/plain", &b""[..]), |f| (f.1, f.2));
            wry::http::Response::builder().header("Content-Type", mime).body(Cow::Borrowed(body)).unwrap()
        })
        .with_ipc_handler(move |request| {
            let _ = page.send_event(UserEvent::Page(request.into_body()));
        })
        .with_background_color((17, 17, 19, 255))
        .with_url("http://macropad.localhost/")
        .build(&window)?;
    let pad = event_loop.create_proxy();
    watch::start(move |signal| {
        let _ = pad.send_event(UserEvent::Pad(signal));
    });

    let keys = key_names();
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        let call = |f: &str, v: Value| {
            let _ = webview.evaluate_script(&format!("{f}({v})"));
        };
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *flow = ControlFlow::Exit,
            Event::UserEvent(UserEvent::Page(body)) => {
                let msg: Value = serde_json::from_str(&body).unwrap_or_default();
                match msg["t"].as_str() {
                    Some("init") => call("init", json!({
                        "slots": load(), "keys": keys, "media": MEDIA, "mouse": MOUSE,
                        "names": SLOTS.map(|s| s.1), "connected": open().is_ok(),
                    })),
                    Some("gravar") => {
                        let text = match serde_json::from_value(msg["slots"].clone()) {
                            Ok(slots) => gravar(&slots),
                            Err(e) => format!("Configuração inválida: {e}"),
                        };
                        call("gravado", json!({ "text": text, "connected": open().is_ok() }));
                    }
                    Some("testar") => run(msg["cmd"].as_str().unwrap_or_default()),
                    Some("led") => {
                        let text = set_led(msg["mode"].as_u64().unwrap_or(0) as u8);
                        call("gravado", json!({ "text": text, "connected": open().is_ok() }));
                    }
                    _ => {}
                }
            }
            Event::UserEvent(UserEvent::Pad(Signal::Plugged(on))) => call("plugged", json!(on)),
            Event::UserEvent(UserEvent::Pad(signal)) => {
                let slots = load();
                call("press", json!({ "slots": hits(&signal, &slots), "what": describe(&signal, &keys) }));
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes conferidos com os testes do modelo 8890 no ch57x-keyboard-tool.
    #[test]
    fn bytes_iguais_aos_da_ferramenta_original() {
        let trim = |m: &[u8; 64]| m[..=m.iter().rposition(|&b| b != 0).unwrap()].to_vec();
        let got = |slot, a| messages(slot, &a).iter().map(trim).collect::<Vec<_>>();
        // A posição 2 (Tecla 3, a da direita) é o id 1; a posição 0 (Tecla 1) é o id 3.
        assert_eq!(
            got(2, Action::Key { mods: 1, code: 0x04 }),
            [vec![3, 0xfe, 1, 1, 1], vec![3, 1, 0x11, 1], vec![3, 1, 0x11, 1, 1, 1, 4], vec![3, 0xaa, 0xaa]]
        );
        assert_eq!(got(1, Action::Media(0xe9))[1], [3, 2, 0x12, 0xe9]);
        assert_eq!(got(0, Action::Mouse { buttons: 1, wheel: 0 })[1], [3, 3, 0x13, 1]);
        assert_eq!(got(3, Action::Mouse { buttons: 0, wheel: 3 })[1], [3, 13, 0x13, 0, 0, 0, 3]);
        // Comando no apertar do knob (posição 4) vira Ctrl+Shift+Alt+F17.
        assert_eq!(got(4, Action::Command("notepad".into()))[2], [3, 14, 0x11, 1, 1, 7, 0x6c]);
        // `led 2`: os mesmos bytes que a ferramenta original manda.
        assert_eq!(led_messages(2).map(|m| trim(&m)), [vec![3, 0xa1, 1], vec![3, 0xb0, 0x18, 2], vec![3, 0xaa, 0xa1]]);
    }

    /// O que a placa manda volta para a posição certa.
    #[test]
    fn reconhece_a_posicao() {
        let slots = [
            Action::Key { mods: 1, code: 0x06 },
            Action::Media(0xcd),
            Action::Command("notepad".into()),
            Action::Media(0xea),
            Action::Mouse { buttons: 0, wheel: 1 },
            Action::Media(0xe9),
        ];
        assert_eq!(hits(&Signal::Key { mods: 1, vk: 0x43 }, &slots), [0]); // Ctrl+C
        assert_eq!(hits(&Signal::Key { mods: 0, vk: 0x43 }, &slots), [] as [usize; 0]);
        assert_eq!(hits(&Signal::Key { mods: 7, vk: 0x7e }, &slots), [2]); // Ctrl+Shift+Alt+F15
        assert_eq!(hits(&Signal::Media(0xe9), &slots), [5]);
        assert_eq!(hits(&Signal::Mouse { buttons: 0, wheel: 1 }, &slots), [4]);
    }
}
