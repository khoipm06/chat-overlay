use gtk4::gdk;
use gtk4::glib;
use gtk4::gsk;
use gtk4::prelude::*;
use std::path::Path;

/// Pumps pending GLib/GTK events so CSS, layout, and popovers settle before capture.
pub fn settle_ui(ms: u64) {
    let ctx = glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    while std::time::Instant::now() < deadline {
        while ctx.pending() {
            ctx.iteration(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
    while ctx.pending() {
        ctx.iteration(false);
    }
}

/// Renders a realized GTK4 window and any visible popovers into a PNG file.
pub fn snapshot_window_with_popovers(
    window: &gtk4::ApplicationWindow,
    popovers: &[(&gtk4::Popover, &gtk4::Widget, bool)],
    out_path: &Path,
) -> Result<(), String> {
    settle_ui(120);

    let width = window.width().max(420);
    let height = window.height().max(800);

    let renderer = gsk::CairoRenderer::new();
    renderer
        .realize(None::<&gdk::Surface>)
        .map_err(|e| format!("CairoRenderer realize failed: {e}"))?;

    let win_paintable = gtk4::WidgetPaintable::new(Some(window));
    let win_snap = gtk4::Snapshot::new();
    win_paintable.snapshot(&win_snap, width as f64, height as f64);
    let win_node = win_snap
        .to_node()
        .ok_or_else(|| "Empty window render node".to_string())?;
    let win_tex = renderer.render_texture(&win_node, None);

    let mut base_bytes = vec![0u8; (width * height * 4) as usize];
    win_tex.download(&mut base_bytes, (width * 4) as usize);

    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)
        .map_err(|e| format!("ImageSurface failed: {e}"))?;
    {
        let cr = cairo::Context::new(&surface).map_err(|e| format!("Cairo context: {e}"))?;
        let win_surf = cairo::ImageSurface::create_for_data(
            base_bytes,
            cairo::Format::ARgb32,
            width,
            height,
            width * 4,
        )
        .map_err(|e| format!("Win surface: {e}"))?;
        cr.set_source_surface(&win_surf, 0.0, 0.0)
            .map_err(|e| format!("Set source: {e}"))?;
        cr.paint().map_err(|e| format!("Paint: {e}"))?;

        for (popover, anchor, above) in popovers {
            if !popover.is_visible() {
                continue;
            }
            let pw = popover.width().max(popover.allocated_width());
            let ph = popover.height().max(popover.allocated_height());
            if pw <= 0 || ph <= 0 {
                continue;
            }
            let pop_paintable = gtk4::WidgetPaintable::new(Some(*popover));
            let pop_snap = gtk4::Snapshot::new();
            pop_paintable.snapshot(&pop_snap, pw as f64, ph as f64);
            if let Some(pop_node) = pop_snap.to_node() {
                let pop_tex = renderer.render_texture(&pop_node, None);
                let mut p_bytes = vec![0u8; (pw * ph * 4) as usize];
                pop_tex.download(&mut p_bytes, (pw * 4) as usize);
                if let Ok(pop_surf) = cairo::ImageSurface::create_for_data(
                    p_bytes,
                    cairo::Format::ARgb32,
                    pw,
                    ph,
                    pw * 4,
                ) {
                    let bounds = anchor.compute_bounds(window);
                    let (ax, ay, aw, ah) = bounds
                        .map(|b| {
                            (
                                b.x() as f64,
                                b.y() as f64,
                                b.width() as f64,
                                b.height() as f64,
                            )
                        })
                        .unwrap_or((16.0, height as f64 - 160.0, 200.0, 30.0));
                    let mut x = (ax + aw - pw as f64).max(8.0);
                    if x + pw as f64 > width as f64 - 8.0 {
                        x = (width as f64 - pw as f64 - 8.0).max(8.0);
                    }
                    let y = if *above {
                        (ay - ph as f64 - 4.0).max(8.0)
                    } else {
                        (ay + ah + 4.0).min((height - ph - 8) as f64)
                    };
                    let _ = cr.set_source_surface(&pop_surf, x, y);
                    let _ = cr.paint();
                }
            }
        }
    }

    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut file =
        std::fs::File::create(out_path).map_err(|e| format!("Create PNG failed: {e}"))?;
    surface
        .write_to_png(&mut file)
        .map_err(|e| format!("Write PNG failed: {e}"))?;
    renderer.unrealize();
    Ok(())
}
