use cardea::app::{App, ButtonKind, Focus};
use cardea::config::{Config, SortColumn, SortDirection};
use cardea::event::AppEvent;
use cardea::theme::Theme;
use cardea::ui;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::path::PathBuf;
use tokio::sync::mpsc::unbounded_channel;

#[tokio::test]
async fn test_headless_ui_rendering() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let theme = Theme::catppuccin_mocha();

    let mut app = App::new(Some(PathBuf::from(".")), &config, tx);

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let res = terminal.draw(|f| {
        ui::render(f, &mut app, &theme);
    });

    assert!(res.is_ok(), "UI rendering failed on headless backend");
}

#[tokio::test]
async fn test_job_queue_toggle_render_and_cancel() {
    let base = std::env::temp_dir().join(format!("cardea_jq_{}", std::process::id()));
    let src = base.join("src");
    let dst = base.join("dst");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    for i in 0..30 {
        std::fs::write(src.join(format!("file_{:02}.txt", i)), "x".repeat(1024)).unwrap();
    }

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // Ctrl+J toggles the queue overlay; empty state renders fine
    app.handle_key_event(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
    assert!(app.show_job_queue);
    let theme = Theme::catppuccin_mocha();
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    let res = terminal.draw(|f| ui::render(f, &mut app, &theme));
    assert!(res.is_ok(), "empty queue rendering failed");

    // Esc closes
    app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.show_job_queue);

    // Submit a real copy job and re-open the queue
    let sources: Vec<PathBuf> = (0..30)
        .map(|i| src.join(format!("file_{:02}.txt", i)))
        .collect();
    app.prepare_transfer(sources, dst.clone(), false);
    assert_eq!(
        app.active_ops.len(),
        1,
        "transfer submitted as a background job"
    );

    app.toggle_job_queue();
    let res = terminal.draw(|f| ui::render(f, &mut app, &theme));
    assert!(res.is_ok(), "queue rendering with an active job failed");

    // Keyboard: cursor movement + cancel request marks the op "cancelling"
    app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // wraps to 0 (single row)
    app.handle_key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    assert!(
        app.active_ops.first().is_some_and(|op| op.cancelling),
        "cancel marks the job"
    );

    // The worker honours the flag at the next item boundary; either outcome
    // (cancelled or already completed) must retire the job cleanly
    for _ in 0..200 {
        pump_events(&mut app, &mut rx).await;
        if app.active_ops.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(app.active_ops.is_empty(), "job removed from the queue");

    // Mouse: click outside the open queue closes it
    app.toggle_job_queue();
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    assert!(!app.show_job_queue, "outside click closes the queue");

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_sidebar_and_breadcrumb_context_menus() {
    let base = std::env::temp_dir().join(format!("cardea_sbcm_{}", std::process::id()));
    let sub = base.join("subfolder");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(base.join("note.txt"), "hi").unwrap();
    std::fs::write(sub.join("inner.txt"), "x").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // --- Sidebar right-click opens a path-targeted menu ---
    // Render once so tree rows record their hit-test paths
    let theme = Theme::catppuccin_mocha();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| ui::render(f, &mut app, &theme)).unwrap();
    assert!(app.sidebar_rect.width > 0, "sidebar visible by default");

    // Right-click the first rendered sidebar row (bookmarks / tree roots —
    // the tree is lazily expanded, so any visible row exercises the wiring)
    let (row_idx, clicked_path) = app
        .sidebar_rendered_paths
        .iter()
        .enumerate()
        .find_map(|(i, p)| p.clone().map(|p| (i, p)))
        .expect("at least one sidebar row rendered");
    let row_y = app.sidebar_rect.y + 1 + (row_idx - app.sidebar_scroll_offset) as u16;
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: app.sidebar_rect.x + 2,
        row: row_y,
        modifiers: KeyModifiers::NONE,
    });
    let menu = app
        .context_menu
        .as_ref()
        .expect("sidebar right-click opens menu");
    assert_eq!(
        menu.target.as_deref(),
        Some(clicked_path.as_path()),
        "menu targets the tree node"
    );
    app.close_context_menu();

    // --- Targeted copy + paste into a folder target ---
    let note = base.join("note.txt");
    app.open_context_menu_for_path(note.clone(), 10, 10);
    app.execute_context_action(cardea::app::ContextAction::Copy);
    assert!(
        app.clipboard
            .as_ref()
            .is_some_and(|c| c.paths.first() == Some(&note)),
        "targeted copy uses the explicit path"
    );

    app.open_context_menu_for_path(sub.clone(), 5, 5);
    let labels: Vec<String> = app
        .context_menu
        .as_ref()
        .unwrap()
        .items
        .iter()
        .map(|i| i.label.clone())
        .collect();
    assert!(
        labels.iter().any(|l| l.contains("Paste Into Folder")),
        "dir menu relabels paste"
    );
    app.execute_context_action(cardea::app::ContextAction::Paste);
    assert!(!app.active_ops.is_empty(), "paste into folder submitted");
    for _ in 0..200 {
        pump_events(&mut app, &mut rx).await;
        if app.active_ops.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        sub.join("note.txt").exists(),
        "clipboard target copied into the folder"
    );
    assert!(app.active_ops.is_empty());

    // --- Targeted rename via the menu action ---
    app.open_context_menu_for_path(note.clone(), 10, 10);
    app.execute_context_action(cardea::app::ContextAction::Rename);
    let dlg = app.dialog.as_ref().expect("rename prompt opens");
    assert!(
        dlg.prompt.as_ref().is_some_and(|p| p.buffer == "note.txt"),
        "prefilled with target name"
    );
    if let Some(dlg) = app.dialog.as_mut() {
        if let Some(p) = dlg.prompt.as_mut() {
            p.buffer = "renamed.txt".to_string();
        }
    }
    app.confirm_dialog();
    pump_events(&mut app, &mut rx).await;
    assert!(
        !note.exists() && base.join("renamed.txt").exists(),
        "targeted rename applied"
    );

    // --- Breadcrumb `m` opens a chip-targeted menu ---

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_syntax_highlighted_preview() {
    let base = std::env::temp_dir().join(format!("cardea_syn_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(
        base.join("main.rs"),
        "fn main() {\n    let greeting = \"hello\";\n    println!(\"{}\", greeting);\n}\n",
    )
    .unwrap();
    // An extension no syntect definition claims → plain fallback
    std::fs::write(base.join("data.xyzq"), "plain contents\n").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let mut config = Config::default();
    config.layout.show_preview = true;
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 2).await;

    // Select main.rs and request its preview
    let idx = app
        .tab()
        .entries
        .iter()
        .position(|e| e.name.as_str() == "main.rs")
        .expect("main.rs listed");
    app.tab_mut().table_selected_index = idx;
    app.maybe_request_preview();

    for _ in 0..200 {
        pump_events(&mut app, &mut rx).await;
        if app.preview_loaded_path.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        app.preview_loaded_path.as_deref(),
        Some(base.join("main.rs").as_path())
    );
    let styled = app
        .preview_styled
        .as_ref()
        .expect(".rs file is syntax-highlighted");
    assert!(!styled.is_empty());
    // The keyword line carries multiple styled regions, not one flat span
    assert!(
        styled.iter().any(|l| l.spans.len() > 1),
        "highlighting produced per-token styling"
    );

    // Renders without panicking
    let theme = Theme::catppuccin_mocha();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let res = terminal.draw(|f| ui::render(f, &mut app, &theme));
    assert!(res.is_ok(), "highlighted preview renders");

    // Unknown extension falls back to plain rendering (no styled variant)
    let idx = app
        .tab()
        .entries
        .iter()
        .position(|e| e.name.as_str() == "data.xyzq")
        .expect("data.xyzq listed");
    app.tab_mut().table_selected_index = idx;
    app.maybe_request_preview();
    for _ in 0..200 {
        pump_events(&mut app, &mut rx).await;
        if app.preview_loaded_path.as_deref() == Some(base.join("data.xyzq").as_path()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        app.preview_loaded_path.as_deref(),
        Some(base.join("data.xyzq").as_path())
    );
    assert!(
        app.preview_styled.is_none(),
        "unknown extension stays plain"
    );

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_image_preview_protocol() {
    let base = std::env::temp_dir().join(format!("cardea_img_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();

    // Generate a small gradient PNG
    let img = image::RgbImage::from_fn(24, 12, |x, y| {
        [(x * 10 % 255) as u8, (y * 20 % 255) as u8, 160u8].into()
    });
    img.save(base.join("gradient.png")).unwrap();
    std::fs::write(base.join("plain.txt"), "not an image\n").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let mut config = Config::default();
    config.layout.show_preview = true;
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 2).await;

    // Select the image and load its preview
    let idx = app
        .tab()
        .entries
        .iter()
        .position(|e| e.name.as_str() == "gradient.png")
        .expect("png listed");
    app.tab_mut().table_selected_index = idx;
    app.maybe_request_preview();
    for _ in 0..200 {
        pump_events(&mut app, &mut rx).await;
        if app.preview_loaded_path.as_deref() == Some(base.join("gradient.png").as_path()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        app.preview_loaded_path.as_deref(),
        Some(base.join("gradient.png").as_path())
    );
    assert!(
        app.preview_image_protocol.is_some(),
        "decoded image produces a graphics-protocol render state"
    );
    assert!(
        app.preview_text.as_ref().is_some_and(|t| t.is_none()),
        "no text payload for images"
    );

    // Renders headlessly via the halfblock fallback backend; metadata header
    // stays visible above the centered image slot
    let theme = Theme::catppuccin_mocha();
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    let res = terminal.draw(|f| ui::render(f, &mut app, &theme));
    assert!(res.is_ok(), "image preview renders on headless backend");
    app.drain_image_encoding();

    let buffer_text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol().to_string())
        .collect();
    assert!(
        buffer_text.contains("Perms:"),
        "metadata visible above image"
    );
    assert!(
        buffer_text.contains("[24\u{d7}12 image]"),
        "image dimensions hint shown"
    );
    // Halfblock rendering fills cells below the header with graphic symbols
    let graphics = buffer_text.chars().filter(|c| "\u{2580}\u{2584}\u{258c}\u{2590}\u{2596}\u{2597}\u{2598}\u{2599}\u{259a}\u{259b}\u{259c}\u{259d}\u{259e}\u{259f}".contains(*c)).count();
    assert!(
        graphics > 2,
        "image pixels rendered as halfblocks, got {}",
        graphics
    );

    // Non-image files leave the protocol cache empty
    let idx = app
        .tab()
        .entries
        .iter()
        .position(|e| e.name.as_str() == "plain.txt")
        .expect("txt listed");
    app.tab_mut().table_selected_index = idx;
    app.maybe_request_preview();
    for _ in 0..200 {
        pump_events(&mut app, &mut rx).await;
        if app.preview_loaded_path.as_deref() == Some(base.join("plain.txt").as_path()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        app.preview_image_protocol.is_none(),
        "text preview clears image state"
    );

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_archive_preview_listing() {
    let base = std::env::temp_dir().join(format!("cardea_arc_{}", std::process::id()));
    let payload = base.join("payload");
    std::fs::create_dir_all(&payload).unwrap();
    std::fs::write(payload.join("alpha.txt"), "alpha contents").unwrap();
    std::fs::write(payload.join("gamma.txt"), "gamma contents").unwrap();

    // .zip (Stored so no deflate feature is needed for writing)
    {
        let file = std::fs::File::create(base.join("bundle.zip")).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        w.start_file("docs/readme.txt", opts).unwrap();
        std::io::Write::write_all(&mut w, b"readme").unwrap();
        w.start_file("beta.log", opts).unwrap();
        std::io::Write::write_all(&mut w, b"log").unwrap();
        w.finish().unwrap();
    }
    // .tar
    {
        let file = std::fs::File::create(base.join("plain.tar")).unwrap();
        let mut b = tar::Builder::new(file);
        b.append_path_with_name(payload.join("alpha.txt"), "alpha.txt")
            .unwrap();
        b.append_path_with_name(payload.join("gamma.txt"), "sub/gamma.txt")
            .unwrap();
        b.into_inner().unwrap();
    }
    // .tar.gz
    {
        let file = std::fs::File::create(base.join("packed.tar.gz")).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut b = tar::Builder::new(enc);
        b.append_path_with_name(payload.join("alpha.txt"), "gz_alpha.txt")
            .unwrap();
        let enc = b.into_inner().unwrap();
        enc.finish().unwrap();
    }
    // .tar.xz
    {
        let file = std::fs::File::create(base.join("tight.tar.xz")).unwrap();
        let enc = xz2::write::XzEncoder::new(file, 1);
        let mut b = tar::Builder::new(enc);
        b.append_path_with_name(payload.join("alpha.txt"), "xz_alpha.txt")
            .unwrap();
        let enc = b.into_inner().unwrap();
        enc.finish().unwrap();
    }
    // .7z
    sevenz_rust::compress_to_path(&payload, base.join("seven.7z")).expect("7z written");

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let mut config = Config::default();
    config.layout.show_preview = true;
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 5).await;

    async fn load_preview_of(
        app: &mut App,
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
        name: &str,
        base: &std::path::Path,
    ) -> String {
        let idx = app
            .tab()
            .entries
            .iter()
            .position(|e| e.name.as_str() == name)
            .unwrap_or_else(|| panic!("{} listed", name));
        app.tab_mut().table_selected_index = idx;
        app.maybe_request_preview();
        for _ in 0..300 {
            pump_events(app, rx).await;
            if app.preview_loaded_path.as_deref() == Some(base.join(name).as_path()) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(
            app.preview_loaded_path.as_deref(),
            Some(base.join(name).as_path())
        );
        app.preview_text.clone().flatten().unwrap_or_default()
    }

    let zip_text = load_preview_of(&mut app, &mut rx, "bundle.zip", &base).await;
    assert!(
        zip_text.contains("docs/readme.txt"),
        "zip lists entry, got: {}",
        zip_text
    );
    assert!(zip_text.contains("beta.log"), "zip lists second entry");

    let tar_text = load_preview_of(&mut app, &mut rx, "plain.tar", &base).await;
    assert!(tar_text.contains("sub/gamma.txt"), "tar lists nested entry");

    let tgz_text = load_preview_of(&mut app, &mut rx, "packed.tar.gz", &base).await;
    assert!(
        tgz_text.contains("gz_alpha.txt"),
        "tar.gz decompresses for listing"
    );

    let txz_text = load_preview_of(&mut app, &mut rx, "tight.tar.xz", &base).await;
    assert!(
        txz_text.contains("xz_alpha.txt"),
        "tar.xz decompresses for listing"
    );

    let sz_text = load_preview_of(&mut app, &mut rx, "seven.7z", &base).await;
    assert!(
        sz_text.contains("alpha.txt"),
        "7z lists entries, got: {}",
        sz_text
    );

    // Nothing was ever extracted to disk besides our own payload dir
    assert!(!base.join("docs").exists());
    assert!(!base.join("sub").exists());

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_binary_hex_dump_preview() {
    let base = std::env::temp_dir().join(format!("cardea_hex_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();

    // A genuinely binary file: full byte range, invalid as UTF-8
    let bytes: Vec<u8> = (0..=255u8).chain([0x00, 0xff, 0x7f, 0x80]).collect();
    std::fs::write(base.join("blob.bin"), &bytes).unwrap();
    // ELF magic prefix to exercise the ASCII gutter
    let mut elf = b"\x7fELF".to_vec();
    elf.extend(std::iter::repeat_n(0u8, 100));
    std::fs::write(base.join("fake.elf"), &elf).unwrap();
    std::fs::write(base.join("text.txt"), "just text\n").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let mut config = Config::default();
    config.layout.show_preview = true;
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 3).await;

    async fn load(
        app: &mut App,
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
        name: &str,
        base: &std::path::Path,
    ) {
        let idx = app
            .tab()
            .entries
            .iter()
            .position(|e| e.name.as_str() == name)
            .unwrap();
        app.tab_mut().table_selected_index = idx;
        app.maybe_request_preview();
        for _ in 0..300 {
            pump_events(app, rx).await;
            if app.preview_loaded_path.as_deref() == Some(base.join(name).as_path()) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("preview for {} never loaded", name);
    }

    load(&mut app, &mut rx, "blob.bin", &base).await;
    assert!(
        app.preview_text.as_ref().is_some_and(|t| t.is_none()),
        "binary has no text payload"
    );
    let dump = app
        .preview_hex
        .clone()
        .expect("binary file gets a hex dump");
    assert!(dump.contains("00000000"), "offset column present");
    assert!(dump.contains("|"), "ASCII gutter present");
    assert!(
        dump.to_lowercase().contains("hex dump"),
        "header describes the dump"
    );

    // Renders headlessly
    let theme = Theme::catppuccin_mocha();
    let mut terminal = Terminal::new(TestBackend::new(110, 35)).unwrap();
    let res = terminal.draw(|f| ui::render(f, &mut app, &theme));
    assert!(res.is_ok(), "hex dump renders");

    // ELF-magic file shows the magic bytes in the gutter
    load(&mut app, &mut rx, "fake.elf", &base).await;
    let dump = app.preview_hex.clone().expect("elf file is binary");
    assert!(
        dump.contains("7f 45 4c 46"),
        "ELF magic hex, got: {}",
        &dump[..dump.len().min(300)]
    );

    // Text files keep plain text and no dump
    load(&mut app, &mut rx, "text.txt", &base).await;
    assert!(app.preview_text.as_ref().is_some_and(|t| t.is_some()));
    assert!(app.preview_hex.is_none(), "text file gets no hex dump");

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_preview_metadata_mime_and_sha256() {
    let base = std::env::temp_dir().join(format!("cardea_meta_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("hello.txt"), "hello world").unwrap();
    std::fs::write(base.join("mystery.xyzunknown"), "data").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let mut config = Config::default();
    config.layout.show_preview = true;
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 2).await;

    async fn load(
        app: &mut App,
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
        name: &str,
        base: &std::path::Path,
    ) {
        let idx = app
            .tab()
            .entries
            .iter()
            .position(|e| e.name.as_str() == name)
            .unwrap();
        app.tab_mut().table_selected_index = idx;
        app.maybe_request_preview();
        for _ in 0..300 {
            pump_events(app, rx).await;
            if app.preview_loaded_path.as_deref() == Some(base.join(name).as_path()) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("preview for {} never loaded", name);
    }

    // hello world → well-known digest; .txt → text/plain
    load(&mut app, &mut rx, "hello.txt", &base).await;
    assert_eq!(
        app.preview_sha256.as_deref(),
        Some("b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"),
        "SHA-256 matches the known digest"
    );
    assert_eq!(app.preview_mime.as_deref(), Some("text/plain"));

    // Renders with both inspector rows
    let theme = Theme::catppuccin_mocha();
    let mut terminal = Terminal::new(TestBackend::new(100, 35)).unwrap();
    let res = terminal.draw(|f| ui::render(f, &mut app, &theme));
    assert!(res.is_ok());
    let buffer_text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol().to_string())
        .collect();
    assert!(buffer_text.contains("SHA256:"), "sha row rendered");
    assert!(buffer_text.contains("text/plain"), "mime row rendered");

    // Unknown extension: no MIME guess, hash still computed
    load(&mut app, &mut rx, "mystery.xyzunknown", &base).await;
    assert!(app.preview_mime.is_none(), "no mime for unknown extension");
    assert!(app.preview_sha256.is_some(), "hash still computed");

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_pager_and_modifier_hint() {
    let base = std::env::temp_dir().join(format!("cardea_pgr_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("doc.txt"), "readable content\n").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let mut config = Config::default();
    config.layout.show_preview = true;
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // --- `p` with a no-op pager reports success without spawning windows ---
    let saved_pager = std::env::var("PAGER").ok();
    std::env::set_var("PAGER", "true");
    let idx = app
        .tab()
        .entries
        .iter()
        .position(|e| e.name.as_str() == "doc.txt")
        .expect("file listed");
    app.tab_mut().table_selected_index = idx;
    app.handle_key_event(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let msg = app
        .status_message
        .as_ref()
        .map(|m| m.text.clone())
        .unwrap_or_default();
    assert!(
        msg.starts_with("Viewing"),
        "pager launch reported, got: {}",
        msg
    );

    // `p` with no $PAGER configured reports guidance
    std::env::remove_var("PAGER");
    app.handle_key_event(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let msg = app
        .status_message
        .as_ref()
        .map(|m| m.text.clone())
        .unwrap_or_default();
    assert!(
        msg.contains("No pager configured"),
        "guidance when PAGER unset, got: {}",
        msg
    );
    match saved_pager {
        Some(v) => std::env::set_var("PAGER", v),
        None => std::env::remove_var("PAGER"),
    }

    // --- Unbound Ctrl+chord opens the modifier discovery popup ---
    app.handle_key_event(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    assert!(app.modifier_hint.is_some(), "unbound Ctrl+chord shows hint");
    let names = app.modifier_hint_names();
    assert_eq!(names, vec!["Ctrl"]);

    // Popup renders and lists known bindings
    let theme = Theme::catppuccin_mocha();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let res = terminal.draw(|f| ui::render(f, &mut app, &theme));
    assert!(res.is_ok());
    let buffer_text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol().to_string())
        .collect();
    assert!(
        buffer_text.contains("Ctrl+keybindings"),
        "popup title shown"
    );
    assert!(buffer_text.contains("Select all"), "Ctrl bindings listed");

    // Any key press dismisses the popup
    app.handle_key_event(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert!(app.modifier_hint.is_none(), "key press dismisses the popup");

    // Bound chords never trigger the popup
    app.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert!(app.modifier_hint.is_none(), "bound chord shows no hint");

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_preview_panel_toggle_rendering() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let mut config = Config::default();
    config.layout.show_preview = true;
    let theme = Theme::tokyo_night();

    let mut app = App::new(Some(PathBuf::from(".")), &config, tx);

    let backend = TestBackend::new(140, 45);
    let mut terminal = Terminal::new(backend).unwrap();

    let res = terminal.draw(|f| {
        ui::render(f, &mut app, &theme);
    });

    assert!(res.is_ok(), "UI rendering failed with preview panel open");
}

#[tokio::test]
async fn test_app_state_navigation_and_shortcuts() {
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(PathBuf::from(".")), &config, tx);

    // Initial state checks
    assert_eq!(app.focus, Focus::MainTable);
    assert_eq!(app.sort_column, SortColumn::Name);
    assert_eq!(app.sort_direction, SortDirection::Ascending);

    // Cycle sort column
    app.handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert_eq!(app.sort_column, SortColumn::Size);

    // Reverse sort direction
    app.handle_key_event(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert_eq!(app.sort_direction, SortDirection::Descending);

    // Open in-place filter
    app.handle_key_event(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::FilterInput);

    // Type in filter
    app.handle_key_event(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    assert_eq!(app.tab().search_query, "md");

    // Close filter with Esc
    app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::MainTable);
    assert_eq!(app.tab().search_query, "");

    // Toggle Preview with F7
    assert!(!app.show_preview);
    app.handle_key_event(KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE));
    assert!(app.show_preview);

    // Toggle Help Modal with '?'
    assert!(!app.show_help);
    app.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    assert!(app.show_help);
    app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.show_help);
}

#[tokio::test]
async fn test_dialog_cancel_flow() {
    let base = std::env::temp_dir().join(format!("cardea_cancel_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("keep_me.txt"), "z").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // `d` opens a trash confirmation dialog
    app.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    assert!(app.dialog.is_some(), "d should open a confirmation dialog");
    // Defaults to Cancel-focused for safety
    let dlg = app.dialog.as_ref().unwrap();
    assert!(matches!(
        dlg.buttons[dlg.selected_button].kind,
        ButtonKind::Cancel
    ));

    // Rendering must succeed with the modal overlay active
    let theme = Theme::gruvbox_dark();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| ui::render(f, &mut app, &theme))
        .expect("dialog rendering failed");

    // Esc cancels
    app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.dialog.is_none());

    // Keys are swallowed while a dialog is open: navigation must not move focus
    app.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert!(app.dialog.is_some());
    assert_eq!(app.focus, Focus::MainTable);
    app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    // Cancel keeps the file untouched
    assert!(base.join("keep_me.txt").exists());
    let _ = std::fs::remove_dir_all(&base);
}

/// Drives all pending events from the channel into app state until at least
/// `min` directory entries are loaded (or the retry budget is exhausted).
async fn wait_for_scan(
    app: &mut App,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    min: usize,
) {
    for _ in 0..200 {
        pump_events(app, rx).await;
        if app.tab().entries.len() >= min {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// Applies every queued event (scan chunks, ops progress/completion) to the
/// app state, mirroring the main loop dispatch.
async fn pump_events(app: &mut App, rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>) {
    while let Ok(ev) = rx.try_recv() {
        match ev {
            AppEvent::DirectoryScannedChunk {
                scan_id,
                path,
                entries,
                is_final,
            } => app.apply_scan_chunk(scan_id, &path, entries, is_final),
            AppEvent::PreviewLoaded {
                path,
                text,
                styled,
                image,
                hex_dump,
                meta,
            } => app.on_preview_loaded(path, text, styled, image, hex_dump, meta),
            AppEvent::OpsProgress {
                job_id,
                done,
                total,
                current,
            } => app.on_ops_progress(job_id, done, total, current),
            AppEvent::OpsFinished {
                job_id,
                label,
                succeeded,
                skipped,
                errors,
                dest,
                cancelled,
            } => app.on_ops_finished(cardea::app::OpsOutcome::from_event(
                job_id, label, succeeded, skipped, errors, dest, cancelled,
            )),
            _ => {}
        }
    }
}

#[tokio::test]
async fn test_dialog_permanent_delete_confirm() {
    let base = std::env::temp_dir().join(format!("cardea_test_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let victim_a = base.join("doomed_a.txt");
    let victim_b = base.join("doomed_b.txt");
    std::fs::write(&victim_a, "x").unwrap();
    std::fs::write(&victim_b, "y").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 2).await;
    assert!(
        app.tab().entries.len() >= 2,
        "scan should list the test files"
    );

    // Select both files
    app.handle_key_event(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(app.tab().multi_selected.len(), 2);

    // Shift+Delete opens a destructive confirmation
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Delete,
        KeyModifiers::SHIFT,
        crossterm::event::KeyEventKind::Press,
    ));
    let dlg = app.dialog.as_ref().expect("Shift+Delete should confirm");
    assert!(dlg.destructive);

    // Right moves button focus to Confirm; Enter executes (async op)
    app.handle_key_event(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.dialog.is_none());

    // Wait for the worker to finish and the post-delete refresh to settle
    for _ in 0..100 {
        pump_events(&mut app, &mut rx).await;
        if !victim_a.exists()
            && !victim_b.exists()
            && app.active_ops.is_empty()
            && !app
                .tab()
                .entries
                .iter()
                .any(|e| e.path == victim_a || e.path == victim_b)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    assert!(!victim_a.exists(), "file should be permanently deleted");
    assert!(!victim_b.exists(), "file should be permanently deleted");
    assert!(
        app.tab().multi_selected.is_empty(),
        "selection of deleted paths cleared"
    );

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_range_and_invert_selection() {
    let base = std::env::temp_dir().join(format!("cardea_sel_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    for name in ["a.txt", "b.txt", "c.txt", "d.txt"] {
        std::fs::write(base.join(name), "z").unwrap();
    }

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 4).await;
    assert_eq!(app.tab().entries.len(), 4);

    // Shift+Down extends the range from the cursor
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Down,
        KeyModifiers::SHIFT,
        crossterm::event::KeyEventKind::Press,
    ));
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Down,
        KeyModifiers::SHIFT,
        crossterm::event::KeyEventKind::Press,
    ));
    assert_eq!(
        app.tab().table_selected_index,
        2,
        "cursor moved to end of range"
    );
    assert_eq!(
        app.tab().multi_selected.len(),
        3,
        "anchor + two extensions selected"
    );

    // Shift+Up extends in the other direction too (no deselection)
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Up,
        KeyModifiers::SHIFT,
        crossterm::event::KeyEventKind::Press,
    ));
    assert_eq!(app.tab().multi_selected.len(), 3);

    // `*` inverts: the one unselected entry becomes the only selection
    app.handle_key_event(KeyEvent::new(KeyCode::Char('*'), KeyModifiers::NONE));
    assert_eq!(app.tab().multi_selected.len(), 1);

    // Esc clears everything including the anchor
    app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.tab().multi_selected.is_empty());
    assert!(app.tab().selection_anchor.is_none());

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_rename_prompt_flow() {
    let base = std::env::temp_dir().join(format!("cardea_ren_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let original = base.join("old_name.txt");
    std::fs::write(&original, "z").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // F2 opens a prompt pre-filled with the current name
    app.handle_key_event(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
    let dlg = app.dialog.as_ref().expect("F2 should open rename prompt");
    let prompt = dlg
        .prompt
        .as_ref()
        .expect("rename dialog has an input line");
    assert_eq!(prompt.buffer, "old_name.txt");

    // Type an extra char at the end, then Enter (Confirm is focused by default)
    app.handle_key_event(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.dialog.is_none());

    for _ in 0..50 {
        pump_events(&mut app, &mut rx).await;
        if base.join("old_name.txt2").exists() && !original.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(!original.exists(), "original file renamed away");
    assert!(base.join("old_name.txt2").exists(), "file renamed");

    // Empty names are rejected and keep the dialog open
    std::fs::write(base.join("tmp_x"), "z").unwrap();
    // Trigger an explicit rescan and wait until BOTH files are listed
    app.refresh();
    for _ in 0..200 {
        pump_events(&mut app, &mut rx).await;
        let have_both = ["old_name.txt2", "tmp_x"]
            .iter()
            .all(|n| app.tab().entries.iter().any(|e| e.name.as_str() == *n));
        if have_both {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    // Move the cursor onto tmp_x ("old_name.txt2" sorts before it)
    app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
    assert_eq!(
        app.dialog
            .as_ref()
            .and_then(|d| d.prompt.as_ref())
            .map(|p| p.buffer.clone()),
        Some("tmp_x".to_string()),
        "prompt should target the cursor's entry"
    );
    for _ in 0..8 {
        app.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    }
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.dialog.is_some(), "empty name keeps dialog open");

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_copy_paste_via_worker() {
    let base = std::env::temp_dir().join(format!("cardea_cpy_{}", std::process::id()));
    let src_dir = base.join("src");
    let dst_dir = base.join("dst");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dst_dir).unwrap();
    std::fs::write(src_dir.join("report.txt"), "hello").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(src_dir.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // Ctrl+C copies, navigate to dst via direct API, Ctrl+V pastes async
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
        crossterm::event::KeyEventKind::Press,
    ));
    assert!(app.clipboard.as_ref().is_some_and(|c| !c.cut));

    app.navigate_to(dst_dir.clone());
    wait_for_scan(&mut app, &mut rx, 0).await;

    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Char('v'),
        KeyModifiers::CONTROL,
        crossterm::event::KeyEventKind::Press,
    ));

    for _ in 0..100 {
        pump_events(&mut app, &mut rx).await;
        if app.active_ops.is_empty() && dst_dir.join("report.txt").exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    assert!(
        dst_dir.join("report.txt").exists(),
        "copied file landed in dst"
    );
    assert!(
        src_dir.join("report.txt").exists(),
        "copy leaves the original intact"
    );
    assert_eq!(
        std::fs::read_to_string(dst_dir.join("report.txt")).unwrap(),
        "hello"
    );

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_context_menu_keyboard_flow() {
    let base = std::env::temp_dir().join(format!("cardea_ctx_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("target.txt"), "z").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // `m` opens the menu with a target present (Open first)
    app.handle_key_event(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
    let menu = app
        .context_menu
        .as_ref()
        .expect("m should open the context menu");
    assert!(menu.items.len() > 5, "menu should list file actions");
    assert!(!menu.items[0].is_separator());

    // Rendering succeeds while the menu is open
    let theme = Theme::nord();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| ui::render(f, &mut app, &theme))
        .expect("context menu rendering failed");

    // Navigate to Copy and execute
    loop {
        let on_copy = app
            .context_menu
            .as_ref()
            .map(|m| m.items[m.selected].label.contains("Copy"))
            .unwrap_or(false);
        if on_copy {
            break;
        }
        app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.context_menu.is_none(),
        "executing an action closes the menu"
    );
    assert!(
        app.clipboard.as_ref().is_some_and(|c| !c.cut),
        "Copy action filled the clipboard"
    );

    // Esc closes without side effects
    app.handle_key_event(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.context_menu.is_none());

    // Input is swallowed while the menu is open
    app.handle_key_event(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Down,
        KeyModifiers::SHIFT,
        crossterm::event::KeyEventKind::Press,
    ));
    assert!(app.context_menu.is_some());
    assert_eq!(
        app.tab().multi_selected.len(),
        0,
        "selection keys ignored while menu open"
    );
    app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_context_menu_right_click_flow() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    let base = std::env::temp_dir().join(format!("cardea_ctxr_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("row_file.txt"), "z").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // Render once so table_rect is populated for hit-testing
    let theme = Theme::catppuccin_mocha();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| ui::render(f, &mut app, &theme)).unwrap();

    // Right-click inside the table body opens the menu on that row
    let row_y = app.table_rect.y + 3;
    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: app.table_rect.x + 10,
        row: row_y,
        modifiers: KeyModifiers::empty(),
    });
    assert!(
        app.context_menu.is_some(),
        "right-click should open the context menu"
    );

    // Clicking a menu item executes it: find the Paste row's y position
    let menu_rect = app.context_menu.as_ref().unwrap().screen_rect;
    let paste_row = app
        .context_menu
        .as_ref()
        .unwrap()
        .items
        .iter()
        .position(|i| i.label.contains("Paste"))
        .unwrap() as u16;
    app.context_menu.as_mut().unwrap().scroll_offset = 0;
    let item_y = menu_rect.y + 1 + paste_row;

    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: menu_rect.x + 2,
        row: item_y,
        modifiers: KeyModifiers::empty(),
    });

    // Paste with an empty clipboard just reports status; menu must be closed
    assert!(app.context_menu.is_none(), "item click closes the menu");

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_drag_and_drop_to_breadcrumb() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    let outer = std::env::temp_dir().join(format!("cardea_dd_{}", std::process::id()));
    let base = outer.join("base");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("portable.txt"), "payload").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // Render once so table_rect and breadcrumb segment areas are populated
    let theme = Theme::catppuccin_mocha();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| ui::render(f, &mut app, &theme)).unwrap();

    // Locate portable.txt's visual row in the table
    let vis_idx = app
        .tab()
        .filtered_indices
        .iter()
        .position(|&idx| app.tab().entries[idx].name == "portable.txt")
        .expect("file visible in table");
    let start_y = app.table_rect.y + 2 + (vis_idx - app.tab().table_scroll_offset) as u16;
    let start_x = app.table_rect.x + 10;

    // Locate the breadcrumb chip for `outer` (parent of current dir)
    let seg = app
        .breadcrumb_segments
        .iter()
        .find(|s| s.path == outer)
        .expect("breadcrumb shows parent dir");
    let drop_x = seg.area.x + seg.area.width / 2;

    // Press, drag past threshold, hover over the chip, release
    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: start_x,
        row: start_y,
        modifiers: KeyModifiers::empty(),
    });
    assert!(app.drag_drop.is_none(), "press alone does not start a drag");

    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: start_x,
        row: start_y.saturating_sub(3),
        modifiers: KeyModifiers::empty(),
    });
    assert!(
        app.drag_drop.is_some(),
        "movement beyond threshold starts a drag"
    );
    assert_eq!(
        app.drag_drop.as_ref().unwrap().paths,
        vec![base.join("portable.txt")]
    );

    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: drop_x,
        row: 1,
        modifiers: KeyModifiers::empty(),
    });

    // Status bar shows live feedback while dragging (render must not panic)
    terminal.draw(|f| ui::render(f, &mut app, &theme)).unwrap();

    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: drop_x,
        row: 1,
        modifiers: KeyModifiers::empty(),
    });
    assert!(app.drag_drop.is_none());

    // Wait for the async move worker to finish
    for _ in 0..100 {
        pump_events(&mut app, &mut rx).await;
        if app.active_ops.is_empty() && outer.join("portable.txt").exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    assert!(
        outer.join("portable.txt").exists(),
        "file moved to drop target"
    );
    assert!(
        !base.join("portable.txt").exists(),
        "source no longer present"
    );

    let _ = std::fs::remove_dir_all(&outer);
}

#[tokio::test]
async fn test_drag_no_op_and_cancel() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    let base = std::env::temp_dir().join(format!("cardea_ddn_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("stay.txt"), "z").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    let theme = Theme::catppuccin_mocha();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| ui::render(f, &mut app, &theme)).unwrap();

    let vis_idx = app
        .tab()
        .filtered_indices
        .iter()
        .position(|&idx| app.tab().entries[idx].name == "stay.txt")
        .unwrap();
    let y = app.table_rect.y + 2 + (vis_idx - app.tab().table_scroll_offset) as u16;

    // Drag released back inside the table cancels silently
    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: app.table_rect.x + 10,
        row: y,
        modifiers: KeyModifiers::empty(),
    });
    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: app.table_rect.x + 30,
        row: y,
        modifiers: KeyModifiers::empty(),
    });
    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: app.table_rect.x + 30,
        row: y,
        modifiers: KeyModifiers::empty(),
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    pump_events(&mut app, &mut rx).await;

    assert!(app.drag_drop.is_none());
    assert!(
        app.active_ops.is_empty(),
        "no job submitted for cancelled drop"
    );
    assert!(base.join("stay.txt").exists(), "file untouched");

    let _ = std::fs::remove_dir_all(&base);
}

// ---- Tab Management (M2) ----

#[tokio::test]
async fn test_tab_lifecycle_and_isolation() {
    let base = std::env::temp_dir().join(format!("cardea_tabs_{}", std::process::id()));
    let sub_a = base.join("alpha");
    let sub_b = base.join("beta");
    std::fs::create_dir_all(&sub_a).unwrap();
    std::fs::create_dir_all(&sub_b).unwrap();
    std::fs::write(sub_a.join("a_file.txt"), "z").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(sub_a.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // Ctrl+T duplicates the current directory into a new active tab
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Char('t'),
        KeyModifiers::CONTROL,
        crossterm::event::KeyEventKind::Press,
    ));
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    assert_eq!(app.tab().current_dir, sub_a);

    // Navigate the new tab away; tab 0 must stay put
    app.navigate_to(sub_b.clone());
    pump_events(&mut app, &mut rx).await;
    assert_eq!(app.tabs[1].current_dir, sub_b);
    assert_eq!(app.tabs[0].current_dir, sub_a);
    assert!(
        app.tabs[1].back_history.contains(&sub_a),
        "history isolated per tab"
    );
    assert!(app.tabs[0].back_history.is_empty());

    // Alt+1 switches back; cached entries survive until the rescan lands
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Char('1'),
        KeyModifiers::ALT,
        crossterm::event::KeyEventKind::Press,
    ));
    assert_eq!(app.active_tab, 0);
    assert_eq!(app.tab().current_dir, sub_a);

    // Ctrl+Tab cycles forward (wraps to tab 1)
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Tab,
        KeyModifiers::CONTROL,
        crossterm::event::KeyEventKind::Press,
    ));
    assert_eq!(app.active_tab, 1);

    // Ctrl+W closes the active tab and activates its neighbor
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Char('w'),
        KeyModifiers::CONTROL,
        crossterm::event::KeyEventKind::Press,
    ));
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.active_tab, 0);
    assert_eq!(app.tab().current_dir, sub_a);
    assert!(!app.should_quit);

    // Closing the last tab quits instead
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Char('w'),
        KeyModifiers::CONTROL,
        crossterm::event::KeyEventKind::Press,
    ));
    assert!(app.should_quit, "closing the final tab quits");
    assert_eq!(app.tabs.len(), 1, "tab list never empties");

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_tab_bar_rendering_and_click_switch() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    let base = std::env::temp_dir().join(format!("cardea_tabbar_{}", std::process::id()));
    let sub = base.join("child");
    std::fs::create_dir_all(&sub).unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // Single tab: no tab bar
    let theme = Theme::catppuccin_mocha();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| ui::render(f, &mut app, &theme)).unwrap();
    assert!(app.tab_chips.is_empty());

    // Open a second tab and navigate it so chip names differ
    app.new_tab();
    app.navigate_to(sub.clone());
    pump_events(&mut app, &mut rx).await;
    terminal.draw(|f| ui::render(f, &mut app, &theme)).unwrap();

    assert_eq!(app.tab_chips.len(), 2, "tab bar shows one chip per tab");

    // Click the first tab's chip to switch back
    let chip_rect = app.tab_chips[0].rect;
    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: chip_rect.x + 1,
        row: chip_rect.y,
        modifiers: KeyModifiers::empty(),
    });
    assert_eq!(app.active_tab, 0);
    assert_eq!(app.tab().current_dir, base);

    let _ = std::fs::remove_dir_all(&base);
}

// ---- Dual-Pane Mode (M2) ----

#[tokio::test]
async fn test_dual_pane_toggle_swap_and_click_focus() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    let base = std::env::temp_dir().join(format!("cardea_dp_{}", std::process::id()));
    let left_dir = base.join("left");
    let right_dir = base.join("right");
    std::fs::create_dir_all(&left_dir).unwrap();
    std::fs::create_dir_all(&right_dir).unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(left_dir.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 0).await;

    // F5/F6 with dual-pane off report instead of transferring
    app.handle_key_event(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE));
    pump_events(&mut app, &mut rx).await;
    assert!(app.clipboard.is_none() && app.pending_transfer.is_none());

    // F3 spawns the second pane at the current directory
    app.handle_key_event(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE));
    assert!(app.dual_pane);
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 0);

    // Navigate pane 1 (right) to `right` via direct API + sync
    app.switch_to_tab(1);
    app.navigate_to(right_dir.clone());
    pump_events(&mut app, &mut rx).await;

    // Tab swaps back to pane 0
    app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.active_tab, 0);
    assert_eq!(app.tab().current_dir, left_dir);

    // Render records both pane rects and routes clicks to the right pane
    let theme = Theme::catppuccin_mocha();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| ui::render(f, &mut app, &theme)).unwrap();
    assert!(app.pane_rects[0].width > 0 && app.pane_rects[1].width > 0);

    let right_center_x = app.pane_rects[1].x + app.pane_rects[1].width / 2;
    let body_y = app.pane_rects[1].y + 4;
    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: right_center_x,
        row: body_y,
        modifiers: KeyModifiers::empty(),
    });
    assert_eq!(app.active_tab, 1, "clicking a pane focuses it");
    assert_eq!(app.tab().current_dir, right_dir);

    // F3 again returns to single-pane view
    app.handle_key_event(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE));
    assert!(!app.dual_pane);

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_dual_pane_f5_f6_cross_pane_transfer() {
    let base = std::env::temp_dir().join(format!("cardea_dp56_{}", std::process::id()));
    let left_dir = base.join("left");
    let right_dir = base.join("right");
    std::fs::create_dir_all(&left_dir).unwrap();
    std::fs::create_dir_all(&right_dir).unwrap();
    std::fs::write(left_dir.join("cargo.txt"), "goods").unwrap();
    std::fs::write(left_dir.join("keep.txt"), "stays").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(left_dir.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 2).await;

    app.handle_key_event(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE));
    app.switch_to_tab(1);
    app.navigate_to(right_dir.clone());
    pump_events(&mut app, &mut rx).await;

    // Back on the left pane; F6 moves cargo.txt to the right pane
    app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.active_tab, 0);

    app.handle_key_event(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE));
    // No collisions -> submitted directly as an async move
    for _ in 0..100 {
        pump_events(&mut app, &mut rx).await;
        if app.active_ops.is_empty() && right_dir.join("cargo.txt").exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        right_dir.join("cargo.txt").exists(),
        "F6 moved file to opposite pane"
    );
    assert!(
        !left_dir.join("cargo.txt").exists(),
        "source gone after move"
    );

    // Switch to the right pane and F5-copy cargo.txt back to the left
    app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.active_tab, 1);
    // Switching tabs kicks off an async rescan; wait for it before acting
    wait_for_scan(&mut app, &mut rx, 1).await;
    app.handle_key_event(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE));
    for _ in 0..100 {
        pump_events(&mut app, &mut rx).await;
        if app.active_ops.is_empty() && left_dir.join("cargo.txt").exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(left_dir.join("cargo.txt").exists(), "F5 copied file back");
    assert!(right_dir.join("cargo.txt").exists(), "copy keeps original");

    let _ = std::fs::remove_dir_all(&base);
}

// ---- Conflict Resolution & Failure Retry (M2) ----

/// Navigates to src, copies the file, navigates to dst, and pastes —
/// returning once the conflict dialog (or transfer) settles.
async fn copy_src_paste_dst(
    app: &mut App,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    src_dir: &std::path::Path,
    dst_dir: &std::path::Path,
) {
    let press = |code| {
        KeyEvent::new_with_kind(
            code,
            KeyModifiers::CONTROL,
            crossterm::event::KeyEventKind::Press,
        )
    };
    app.navigate_to(src_dir.to_path_buf());
    wait_for_scan(app, rx, 1).await;
    app.handle_key_event(press(KeyCode::Char('c')));
    app.navigate_to(dst_dir.to_path_buf());
    wait_for_scan(app, rx, 1).await;
    app.handle_key_event(press(KeyCode::Char('v')));
}

#[tokio::test]
async fn test_transfer_conflict_dialog_resolutions() {
    let base = std::env::temp_dir().join(format!("cardea_conf_{}", std::process::id()));
    let src_dir = base.join("src");
    let dst_dir = base.join("dst");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dst_dir).unwrap();
    std::fs::write(src_dir.join("file.txt"), "new content").unwrap();
    std::fs::write(dst_dir.join("file.txt"), "old content").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(src_dir.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // Copy file.txt and paste over the existing one: conflict dialog opens
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
        crossterm::event::KeyEventKind::Press,
    ));
    app.navigate_to(dst_dir.clone());
    wait_for_scan(&mut app, &mut rx, 1).await;
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Char('v'),
        KeyModifiers::CONTROL,
        crossterm::event::KeyEventKind::Press,
    ));

    let dlg = app
        .dialog
        .as_ref()
        .expect("collision should open conflict dialog");
    assert_eq!(dlg.buttons.len(), 4, "Overwrite/Skip/Auto-Rename/Cancel");
    assert!(matches!(
        dlg.buttons[dlg.selected_button].kind,
        ButtonKind::Cancel
    ));

    // Esc cancels: nothing transferred, pending transfer discarded
    app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(
        app.pending_transfer.is_none(),
        "cancel discards the queued transfer"
    );
    assert_eq!(
        std::fs::read_to_string(dst_dir.join("file.txt")).unwrap(),
        "old content"
    );

    // Paste again -> Skip keeps the destination untouched
    copy_src_paste_dst(&mut app, &mut rx, &src_dir, &dst_dir).await;
    // selected=3(Cancel); Left x2 -> 1(Skip); Enter
    app.handle_key_event(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    for _ in 0..50 {
        pump_events(&mut app, &mut rx).await;
        if app.active_ops.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        std::fs::read_to_string(dst_dir.join("file.txt")).unwrap(),
        "old content",
        "Skip leaves the destination untouched"
    );
    assert!(!dst_dir.join("file (2).txt").exists());

    // Paste again -> Auto-Rename writes "file (2).txt"
    copy_src_paste_dst(&mut app, &mut rx, &src_dir, &dst_dir).await;
    // selected=3; Left -> 2(Auto-Rename); Enter
    app.handle_key_event(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    for _ in 0..100 {
        pump_events(&mut app, &mut rx).await;
        if app.active_ops.is_empty() && dst_dir.join("file (2).txt").exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        std::fs::read_to_string(dst_dir.join("file (2).txt")).unwrap(),
        "new content",
        "Auto-Rename lands under a fresh name"
    );

    // Paste again -> Tab wraps to Overwrite; destination replaced
    copy_src_paste_dst(&mut app, &mut rx, &src_dir, &dst_dir).await;
    app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)); // 3 -> 0
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    for _ in 0..100 {
        pump_events(&mut app, &mut rx).await;
        if app.active_ops.is_empty()
            && std::fs::read_to_string(dst_dir.join("file.txt")).is_ok_and(|c| c == "new content")
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        std::fs::read_to_string(dst_dir.join("file.txt")).unwrap(),
        "new content",
        "Overwrite replaces the destination"
    );
    assert!(src_dir.join("file.txt").exists(), "copy keeps the source");

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_ops_failure_opens_retry_dialog() {
    let base = std::env::temp_dir().join(format!("cardea_retry_{}", std::process::id()));
    let src_dir = base.join("src");
    let dst_dir = base.join("dst");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dst_dir).unwrap();
    let failed_file = src_dir.join("doomed.txt");
    std::fs::write(&failed_file, "retry me").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(src_dir.clone()), &config, tx);

    // Simulate a batch where one item failed mid-transfer
    app.on_ops_finished(cardea::app::OpsOutcome::from_event(
        42,
        "Copied".to_string(),
        2,
        0,
        vec![(failed_file.clone(), "Permission denied".to_string())],
        Some(dst_dir.clone()),
        false,
    ));

    // A retry dialog opens offering the failed subset
    let dlg = app
        .dialog
        .as_ref()
        .expect("failures should open retry dialog");
    assert!(matches!(
        dlg.action,
        cardea::app::DialogAction::RetryTransfer { .. }
    ));
    assert!(matches!(
        dlg.buttons[dlg.selected_button].kind,
        ButtonKind::Confirm
    ));
    assert!(dlg.message.iter().any(|l| l.contains("Permission denied")));

    // Enter retries: the failed item is resubmitted to the same destination
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.dialog.is_none());
    for _ in 0..100 {
        pump_events(&mut app, &mut rx).await;
        if app.active_ops.is_empty() && dst_dir.join("doomed.txt").exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        std::fs::read_to_string(dst_dir.join("doomed.txt")).unwrap(),
        "retry me",
        "retry completes the previously failed item"
    );

    let _ = std::fs::remove_dir_all(&base);
}

// ---- Create Folder / Create File ----

#[tokio::test]
async fn test_create_folder_via_context_menu_and_file_via_keys() {
    let base = std::env::temp_dir().join(format!("cardea_new_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("seed.txt"), "z").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // Open the context menu on an empty-cursor directory listing: New Folder
    // must be present even without a target entry under the cursor
    app.handle_key_event(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
    loop {
        let on_it = app
            .context_menu
            .as_ref()
            .map(|m| m.items[m.selected].label.contains("New Folder"))
            .unwrap_or(false);
        if on_it {
            break;
        }
        let more = app
            .context_menu
            .as_ref()
            .map(|m| m.selected + 1 < m.items.len())
            .unwrap_or(false);
        assert!(more, "context menu should contain a New Folder entry");
        app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // Prompt opens; type the name and confirm
    let dlg = app.dialog.as_ref().expect("New Folder opens a prompt");
    assert!(matches!(
        dlg.action,
        cardea::app::DialogAction::CreateFolder(_)
    ));
    for c in "fresh_dir".chars() {
        app.handle_key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.dialog.is_none());

    for _ in 0..50 {
        pump_events(&mut app, &mut rx).await;
        if base.join("fresh_dir").is_dir() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        base.join("fresh_dir").is_dir(),
        "folder created via context menu"
    );

    // Ctrl+Shift+N creates an empty file ('N' terminal encoding)
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Char('N'),
        KeyModifiers::CONTROL,
        crossterm::event::KeyEventKind::Press,
    ));
    let dlg = app
        .dialog
        .as_ref()
        .expect("Ctrl+Shift+N opens new-file prompt");
    assert!(matches!(
        dlg.action,
        cardea::app::DialogAction::CreateFile(_)
    ));
    for c in "notes.txt".chars() {
        app.handle_key_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    for _ in 0..50 {
        pump_events(&mut app, &mut rx).await;
        if base.join("notes.txt").exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        base.join("notes.txt").is_file(),
        "file created via Ctrl+Shift+N"
    );
    assert_eq!(
        std::fs::read(base.join("notes.txt")).unwrap(),
        Vec::<u8>::new()
    );

    let _ = std::fs::remove_dir_all(&base);
}

// ---- Desktop Integration: User Actions, Terminal, Editor ----

#[tokio::test]
async fn test_user_action_executes_via_key_and_context_menu() {
    use cardea::config::UserAction;

    let base = std::env::temp_dir().join(format!("cardea_act_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("input.txt"), "payload").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let mut config = Config::default();
    // Real command: `cp {file} copied.txt` — observable side effect
    config.actions.push(UserAction {
        name: "Duplicate File".to_string(),
        key: Some("ctrl+g".to_string()),
        command: "cp".to_string(),
        args: vec!["{file}".to_string(), "{dir}/copied.txt".to_string()],
    });

    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // Keybinding dispatch: Ctrl+G runs the action with the cursor's file
    app.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Char('g'),
        KeyModifiers::CONTROL,
        crossterm::event::KeyEventKind::Press,
    ));
    for _ in 0..50 {
        if base.join("copied.txt").exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        base.join("copied.txt").exists(),
        "{{file}} placeholder expanded and cp ran"
    );
    assert_eq!(
        std::fs::read_to_string(base.join("copied.txt")).unwrap(),
        "payload"
    );
    assert!(app
        .status_message
        .as_ref()
        .is_some_and(|m| m.text.contains("Duplicate File")));

    // Context menu lists the action by name; selecting it re-runs it
    std::fs::remove_file(base.join("copied.txt")).unwrap();
    app.handle_key_event(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
    loop {
        let on_it = app
            .context_menu
            .as_ref()
            .map(|m| m.items[m.selected].label.contains("Duplicate File"))
            .unwrap_or(false);
        if on_it {
            break;
        }
        let more = app
            .context_menu
            .as_ref()
            .map(|m| m.selected + 1 < m.items.len())
            .unwrap_or(false);
        assert!(more, "user action should appear in the context menu");
        app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.context_menu.is_none());

    for _ in 0..50 {
        if base.join("copied.txt").exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        base.join("copied.txt").exists(),
        "menu activation executes the action"
    );

    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn test_open_terminal_and_editor_paths() {
    let base = std::env::temp_dir().join(format!("cardea_dti_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("doc.txt"), "hello").unwrap();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let config = Config::default();
    let mut app = App::new(Some(base.clone()), &config, tx);
    wait_for_scan(&mut app, &mut rx, 1).await;

    // Never spawn real windows during tests: point $TERMINAL/$EDITOR at a
    // harmless no-op binary so spawn() succeeds without opening anything.
    let saved_terminal = std::env::var("TERMINAL").ok();
    std::env::set_var("TERMINAL", "true");

    // F4 opens a terminal via $TERMINAL — verify success feedback
    app.handle_key_event(KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE));
    let msg = app
        .status_message
        .as_ref()
        .map(|m| m.text.clone())
        .unwrap_or_default();
    assert!(
        msg.contains("Terminal opened"),
        "no-op terminal reported as opened, got: {}",
        msg
    );

    // `e` with a no-op editor reports success without launching a GUI
    let saved_editor = std::env::var("EDITOR").ok();
    let saved_visual = std::env::var("VISUAL").ok();
    std::env::remove_var("VISUAL");
    std::env::set_var("EDITOR", "true");
    app.handle_key_event(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    let msg = app
        .status_message
        .as_ref()
        .map(|m| m.text.clone())
        .unwrap_or_default();
    assert!(
        msg.starts_with("Opened") || msg.starts_with("Editing"),
        "editor attempt reported, got: {}",
        msg
    );

    // Restore the caller's environment (tests share one process)
    match saved_terminal {
        Some(v) => std::env::set_var("TERMINAL", v),
        None => std::env::remove_var("TERMINAL"),
    }

    // `e` with no $EDITOR/$VISUAL set reports a helpful error instead of
    // failing silently (CI/headless shells often have no editor)
    let had_editor = saved_editor
        .as_deref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
        || saved_visual
            .as_deref()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
    if had_editor {
        std::env::remove_var("EDITOR");
        std::env::remove_var("VISUAL");
        app.handle_key_event(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        let msg = app
            .status_message
            .as_ref()
            .map(|m| m.text.clone())
            .unwrap_or_default();
        assert!(
            msg.contains("No editor configured"),
            "guidance when EDITOR unset, got: {}",
            msg
        );
    }

    // Restore editor env last so nothing else observes the cleared state
    match (saved_editor, saved_visual) {
        (Some(e), v) => {
            std::env::set_var("EDITOR", e);
            match v {
                Some(v) => std::env::set_var("VISUAL", v),
                None => std::env::remove_var("VISUAL"),
            }
        }
        (None, Some(v)) => {
            std::env::remove_var("EDITOR");
            std::env::set_var("VISUAL", v);
        }
        (None, None) => {
            std::env::remove_var("EDITOR");
            std::env::remove_var("VISUAL");
        }
    }

    let _ = std::fs::remove_dir_all(&base);
}
