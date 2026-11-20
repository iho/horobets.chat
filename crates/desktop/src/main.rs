mod state;
mod ui;
use gpui::*;
use gpui_component::{Root, Theme, ThemeMode};
use state::{CloseOverlay, Messenger, QuickSearch, Quit};
fn main() -> anyhow::Result<()> {
    let directory = std::env::var_os("MESSENGER_CLIENT_DATA_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            directories::ProjectDirs::from("chat", "horobets", "Messenger")
                .map(|p| p.data_local_dir().to_owned())
        })
        .ok_or_else(|| {
            anyhow::anyhow!("Cannot locate app data directory; set MESSENGER_CLIENT_DATA_DIR")
        })?;
    let layout_path = directory.join("layout.json");
    let engine = client_core::ClientEngine::start(directory)?;
    Application::new()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            cx.bind_keys([
                KeyBinding::new("cmd-k", QuickSearch, Some("Messenger")),
                KeyBinding::new("ctrl-k", QuickSearch, Some("Messenger")),
                KeyBinding::new("escape", CloseOverlay, Some("Messenger")),
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("ctrl-q", Quit, None),
                KeyBinding::new(
                    "shift-enter",
                    gpui_component::input::Enter { secondary: true },
                    Some("Input"),
                ),
            ]);
            cx.on_action(|_: &Quit, cx| cx.quit());
            let bounds = Bounds::centered(None, size(px(1320.), px(860.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(1000.), px(640.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("horobets.chat".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    let view = cx.new(|cx| Messenger::new(engine, layout_path, window, cx));
                    view.read(cx).focus.focus(window);
                    ui::references::install(&view, cx);
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("open desktop window");
            cx.activate(true);
        });
    Ok(())
}
