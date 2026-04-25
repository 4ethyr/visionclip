#[cfg(feature = "gtk-overlay")]
pub const GTK_OVERLAY_ENABLED: bool = true;
#[cfg(not(feature = "gtk-overlay"))]
pub const GTK_OVERLAY_ENABLED: bool = false;

#[cfg(feature = "gtk-overlay")]
mod imp {
    use anyhow::Result;
    use gtk::cairo;
    use gtk::glib::{self, ControlFlow};
    use gtk::prelude::*;
    use gtk4 as gtk;
    use std::f64::consts::{FRAC_PI_2, PI, TAU};
    use std::time::{Duration, Instant};

    const CANVAS_SIZE: i32 = 220;
    const CONTAINER_SIZE: f64 = 150.0;
    const BLOB_SIZE: f64 = 100.0;
    const GLASS_SIZE: f64 = 60.0;
    const BLUE: (f64, f64, f64) = (66.0 / 255.0, 133.0 / 255.0, 244.0 / 255.0);
    const PURPLE: (f64, f64, f64) = (161.0 / 255.0, 75.0 / 255.0, 1.0);
    const PINK: (f64, f64, f64) = (1.0, 75.0 / 255.0, 145.0 / 255.0);

    pub fn run_listening_overlay(duration_ms: u64) -> Result<()> {
        let app = gtk::Application::builder()
            .application_id("io.visionclip.overlay")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();

        app.connect_activate(move |app| {
            install_overlay_css();

            let window = gtk::ApplicationWindow::builder()
                .application(app)
                .title("VisionClip Listening")
                .decorated(false)
                .resizable(false)
                .default_width(CANVAS_SIZE)
                .default_height(CANVAS_SIZE)
                .build();
            window.add_css_class("voice-overlay-root");
            window.set_focusable(false);
            window.set_can_target(false);

            let area = gtk::DrawingArea::builder()
                .content_width(CANVAS_SIZE)
                .content_height(CANVAS_SIZE)
                .halign(gtk::Align::Center)
                .valign(gtk::Align::Center)
                .build();
            area.add_css_class("voice-overlay-canvas");

            let started_at = Instant::now();
            area.set_draw_func(move |_, cr, width, height| {
                draw_overlay_frame(cr, width as f64, height as f64, started_at.elapsed());
            });

            let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
            container.add_css_class("voice-overlay-window");
            container.set_size_request(CANVAS_SIZE, CANVAS_SIZE);
            container.set_hexpand(false);
            container.set_vexpand(false);
            container.set_halign(gtk::Align::Center);
            container.set_valign(gtk::Align::Center);
            container.append(&area);

            window.set_child(Some(&container));
            window.present();

            glib::timeout_add_local(Duration::from_millis(16), move || {
                area.queue_draw();
                ControlFlow::Continue
            });

            let app = app.clone();
            glib::timeout_add_local_once(Duration::from_millis(duration_ms.max(300)), move || {
                app.quit();
            });
        });

        let args = ["visionclip-overlay"];
        let _ = app.run_with_args(&args);
        Ok(())
    }

    fn install_overlay_css() {
        let provider = gtk::CssProvider::new();
        provider.load_from_data(
            "
            window,
            window > contents,
            window.background,
            window.background > contents,
            window.voice-overlay-root,
            window.voice-overlay-root > contents,
            window.background.voice-overlay-root,
            window.background.voice-overlay-root > contents,
            window.background.voice-overlay-root:backdrop,
            window.background.voice-overlay-root:backdrop > contents,
            .voice-overlay-window {
                background: transparent;
                background-color: rgba(0, 0, 0, 0);
                background-image: none;
                box-shadow: none;
                border: none;
            }

            .voice-overlay-canvas {
                background: transparent;
                background-color: rgba(0, 0, 0, 0);
                background-image: none;
            }
            ",
        );

        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    fn draw_overlay_frame(cr: &cairo::Context, width: f64, height: f64, elapsed: Duration) {
        cr.save().ok();
        cr.set_operator(cairo::Operator::Source);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        cr.paint().ok();
        cr.set_operator(cairo::Operator::Over);
        cr.restore().ok();

        let t = elapsed.as_secs_f64();
        let cx = width / 2.0;
        let cy = height / 2.0;

        cr.save().ok();
        cr.translate(cx, cy);

        draw_glow(cr, t);
        draw_blob(cr, t);
        draw_glass_circle(cr, t);

        cr.restore().ok();
    }

    fn draw_glow(cr: &cairo::Context, t: f64) {
        let phase = ping_pong_progress(t / 3.0);
        let scale = lerp(1.0, 1.2, phase);
        let opacity = lerp(0.5, 0.8, phase);
        let radius = (CONTAINER_SIZE / 2.0) * scale;
        let glow = cairo::RadialGradient::new(0.0, 0.0, radius * 0.06, 0.0, 0.0, radius);
        glow.add_color_stop_rgba(0.0, BLUE.0, BLUE.1, BLUE.2, 0.4 * opacity);
        glow.add_color_stop_rgba(0.4, PURPLE.0, PURPLE.1, PURPLE.2, 0.3 * opacity);
        glow.add_color_stop_rgba(0.7, PURPLE.0, PURPLE.1, PURPLE.2, 0.0);
        glow.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);

        cr.save().ok();
        cr.arc(0.0, 0.0, radius, 0.0, TAU);
        cr.set_source(&glow).ok();
        cr.fill().ok();
        cr.restore().ok();
    }

    fn draw_blob(cr: &cairo::Context, t: f64) {
        let state = blob_state_at(t);
        let blur_factor = state.blur / 20.0;
        let layers = [
            (1.0 + blur_factor * 0.45, 0.05),
            (1.0 + blur_factor * 0.35, 0.08),
            (1.0 + blur_factor * 0.25, 0.12),
            (1.0 + blur_factor * 0.15, 0.20),
            (1.0 + blur_factor * 0.05, 0.30),
            (1.0, 0.60),
        ];

        cr.save().ok();
        cr.rotate(state.rotation);
        cr.scale(state.scale, state.scale);

        for (size_scale, alpha) in layers {
            draw_blob_path(cr, BLOB_SIZE * size_scale, &state.radii_x, &state.radii_y);
            cr.set_source(&blob_gradient(BLOB_SIZE * size_scale, alpha))
                .ok();
            cr.fill().ok();
        }

        cr.restore().ok();
    }

    fn blob_gradient(size: f64, alpha: f64) -> cairo::LinearGradient {
        let half = size / 2.0;
        let gradient = cairo::LinearGradient::new(-half, half, half, -half);
        gradient.add_color_stop_rgba(0.0, BLUE.0, BLUE.1, BLUE.2, alpha);
        gradient.add_color_stop_rgba(0.5, PURPLE.0, PURPLE.1, PURPLE.2, alpha);
        gradient.add_color_stop_rgba(1.0, PINK.0, PINK.1, PINK.2, alpha);
        gradient
    }

    fn draw_blob_path(cr: &cairo::Context, size: f64, radii_x: &[f64; 4], radii_y: &[f64; 4]) {
        let width = size;
        let height = size;
        let half_w = width / 2.0;
        let half_h = height / 2.0;
        let left = -half_w;
        let right = half_w;
        let top = -half_h;
        let bottom = half_h;
        let (rx, ry) = normalized_corner_radii(width, height, radii_x, radii_y);

        cr.new_path();
        cr.move_to(left + rx[0], top);
        cr.line_to(right - rx[1], top);
        append_ellipse_arc(
            cr,
            right - rx[1],
            top + ry[1],
            rx[1],
            ry[1],
            -FRAC_PI_2,
            0.0,
        );
        cr.line_to(right, bottom - ry[2]);
        append_ellipse_arc(
            cr,
            right - rx[2],
            bottom - ry[2],
            rx[2],
            ry[2],
            0.0,
            FRAC_PI_2,
        );
        cr.line_to(left + rx[3], bottom);
        append_ellipse_arc(
            cr,
            left + rx[3],
            bottom - ry[3],
            rx[3],
            ry[3],
            FRAC_PI_2,
            PI,
        );
        cr.line_to(left, top + ry[0]);
        append_ellipse_arc(
            cr,
            left + rx[0],
            top + ry[0],
            rx[0],
            ry[0],
            PI,
            PI + FRAC_PI_2,
        );
        cr.close_path();
    }

    fn draw_glass_circle(cr: &cairo::Context, t: f64) {
        let radius = GLASS_SIZE / 2.0;
        let wave_offset = -GLASS_SIZE * ((t / 1.5).rem_euclid(1.0));

        draw_glass_shadow(cr, radius);

        cr.push_group();

        cr.arc(0.0, 0.0, radius, 0.0, TAU);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.15);
        cr.fill_preserve().ok();
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.20);
        cr.set_line_width(1.0);
        cr.stroke().ok();

        cr.save().ok();
        cr.arc(0.0, 0.0, radius, 0.0, TAU);
        cr.clip();
        cr.set_operator(cairo::Operator::DestOut);
        cr.set_source_rgba(0.0, 0.0, 0.0, 1.0);
        draw_wave_cutout(cr, wave_offset, radius);
        cr.restore().ok();

        cr.pop_group_to_source().ok();
        cr.paint().ok();
    }

    fn draw_glass_shadow(cr: &cairo::Context, radius: f64) {
        let shadow = cairo::RadialGradient::new(0.0, 4.0, radius * 0.3, 0.0, 4.0, radius * 1.6);
        shadow.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.10);
        shadow.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
        cr.save().ok();
        cr.arc(0.0, 4.0, radius * 1.2, 0.0, TAU);
        cr.set_source(&shadow).ok();
        cr.fill().ok();
        cr.restore().ok();
    }

    fn draw_wave_cutout(cr: &cairo::Context, offset: f64, radius: f64) {
        let baseline = 0.0;
        let segment = GLASS_SIZE / 2.0;
        let start = -radius * 3.0 + offset;
        let segments = 10;

        cr.new_path();
        cr.set_line_width(6.0);
        cr.set_line_cap(cairo::LineCap::Round);
        for idx in 0..segments {
            let x0 = start + idx as f64 * segment;
            let x1 = x0 + segment;
            let cx = x0 + segment / 2.0;
            let cy = if idx % 2 == 0 { -15.0 } else { 15.0 };
            if idx == 0 {
                cr.move_to(x0, baseline);
            }
            append_quadratic_curve(cr, cx, cy, x1, baseline);
        }
        cr.stroke().ok();
    }

    fn append_quadratic_curve(cr: &cairo::Context, cx: f64, cy: f64, x: f64, y: f64) {
        let (x0, y0) = cr.current_point().unwrap_or((0.0, 0.0));
        let c1x = x0 + (2.0 / 3.0) * (cx - x0);
        let c1y = y0 + (2.0 / 3.0) * (cy - y0);
        let c2x = x + (2.0 / 3.0) * (cx - x);
        let c2y = y + (2.0 / 3.0) * (cy - y);
        cr.curve_to(c1x, c1y, c2x, c2y, x, y);
    }

    fn append_ellipse_arc(
        cr: &cairo::Context,
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        start: f64,
        end: f64,
    ) {
        cr.save().ok();
        cr.translate(cx, cy);
        cr.scale(rx.max(0.0001), ry.max(0.0001));
        cr.arc(0.0, 0.0, 1.0, start, end);
        cr.restore().ok();
    }

    fn normalized_corner_radii(
        width: f64,
        height: f64,
        radii_x: &[f64; 4],
        radii_y: &[f64; 4],
    ) -> ([f64; 4], [f64; 4]) {
        let mut rx = [
            width * radii_x[0] / 100.0,
            width * radii_x[1] / 100.0,
            width * radii_x[2] / 100.0,
            width * radii_x[3] / 100.0,
        ];
        let mut ry = [
            height * radii_y[0] / 100.0,
            height * radii_y[1] / 100.0,
            height * radii_y[2] / 100.0,
            height * radii_y[3] / 100.0,
        ];

        let scale = [
            width / (rx[0] + rx[1]).max(width),
            width / (rx[3] + rx[2]).max(width),
            height / (ry[0] + ry[3]).max(height),
            height / (ry[1] + ry[2]).max(height),
        ]
        .into_iter()
        .fold(1.0, f64::min)
        .min(1.0);

        for value in &mut rx {
            *value *= scale;
        }
        for value in &mut ry {
            *value *= scale;
        }

        (rx, ry)
    }

    fn blob_state_at(t: f64) -> BlobState {
        let cycle = (t / 4.0).rem_euclid(1.0);
        let (from, to, local_t) = if cycle < 0.25 {
            (BLOB_KEYFRAMES[0], BLOB_KEYFRAMES[1], cycle / 0.25)
        } else if cycle < 0.5 {
            (BLOB_KEYFRAMES[1], BLOB_KEYFRAMES[2], (cycle - 0.25) / 0.25)
        } else if cycle < 0.75 {
            (BLOB_KEYFRAMES[2], BLOB_KEYFRAMES[3], (cycle - 0.5) / 0.25)
        } else {
            (BLOB_KEYFRAMES[3], BLOB_KEYFRAMES[4], (cycle - 0.75) / 0.25)
        };

        let eased = smoothstep(local_t);
        BlobState {
            rotation: lerp(from.rotation, to.rotation, eased),
            scale: lerp(from.scale, to.scale, eased),
            blur: lerp(from.blur, to.blur, eased),
            radii_x: lerp_radii(from.radii_x, to.radii_x, eased),
            radii_y: lerp_radii(from.radii_y, to.radii_y, eased),
        }
    }

    fn lerp_radii(from: [f64; 4], to: [f64; 4], t: f64) -> [f64; 4] {
        [
            lerp(from[0], to[0], t),
            lerp(from[1], to[1], t),
            lerp(from[2], to[2], t),
            lerp(from[3], to[3], t),
        ]
    }

    fn ping_pong_progress(t: f64) -> f64 {
        let cycle = t.rem_euclid(1.0);
        if cycle <= 0.5 {
            smoothstep(cycle / 0.5)
        } else {
            smoothstep((1.0 - cycle) / 0.5)
        }
    }

    fn smoothstep(t: f64) -> f64 {
        let clamped = t.clamp(0.0, 1.0);
        clamped * clamped * (3.0 - 2.0 * clamped)
    }

    fn lerp(from: f64, to: f64, t: f64) -> f64 {
        from + (to - from) * t
    }

    #[derive(Clone, Copy)]
    struct BlobKeyframe {
        rotation: f64,
        scale: f64,
        blur: f64,
        radii_x: [f64; 4],
        radii_y: [f64; 4],
    }

    #[derive(Clone, Copy)]
    struct BlobState {
        rotation: f64,
        scale: f64,
        blur: f64,
        radii_x: [f64; 4],
        radii_y: [f64; 4],
    }

    const BLOB_KEYFRAMES: [BlobKeyframe; 5] = [
        BlobKeyframe {
            rotation: 0.0,
            scale: 0.8,
            blur: 20.0,
            radii_x: [60.0, 40.0, 30.0, 70.0],
            radii_y: [60.0, 30.0, 70.0, 40.0],
        },
        BlobKeyframe {
            rotation: PI * 0.5,
            scale: 1.05,
            blur: 14.0,
            radii_x: [30.0, 60.0, 70.0, 40.0],
            radii_y: [50.0, 60.0, 30.0, 60.0],
        },
        BlobKeyframe {
            rotation: PI,
            scale: 1.3,
            blur: 8.0,
            radii_x: [70.0, 30.0, 50.0, 50.0],
            radii_y: [30.0, 40.0, 60.0, 70.0],
        },
        BlobKeyframe {
            rotation: PI * 1.5,
            scale: 1.05,
            blur: 14.0,
            radii_x: [40.0, 70.0, 40.0, 60.0],
            radii_y: [60.0, 50.0, 40.0, 30.0],
        },
        BlobKeyframe {
            rotation: TAU,
            scale: 0.8,
            blur: 20.0,
            radii_x: [60.0, 40.0, 30.0, 70.0],
            radii_y: [60.0, 30.0, 70.0, 40.0],
        },
    ];
}

#[cfg(feature = "gtk-overlay")]
pub use imp::run_listening_overlay;

pub fn is_overlay_available() -> bool {
    GTK_OVERLAY_ENABLED
}

#[cfg(not(feature = "gtk-overlay"))]
pub fn run_listening_overlay(_duration_ms: u64) -> anyhow::Result<()> {
    anyhow::bail!("visionclip was built without the `gtk-overlay` feature")
}
