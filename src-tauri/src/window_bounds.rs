use serde::{Deserialize, Serialize};
use std::future::Future;
use tauri::{LogicalSize, Monitor, PhysicalPosition, PhysicalSize, Position, Size};

const BASE_WIDTH_LOGICAL: f64 = 1440.0;
const BASE_HEIGHT_LOGICAL: f64 = 938.0;
const MIN_WIDTH_LOGICAL: f64 = 960.0;
const MIN_HEIGHT_LOGICAL: f64 = 600.0;
const WORK_AREA_FRACTION: f64 = 1.0;
const AUTO_WORK_AREA_MARGIN_LOGICAL: f64 = 24.0;
const ZOOM_CHANGE_EPSILON: f64 = 0.001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScaleRequest {
    pub(crate) preference: UiScalePreference,
    pub(crate) revision: u64,
    pub(crate) preference_revision: u64,
    pub(crate) force_geometry_revision: u64,
}

impl Default for ScaleRequest {
    fn default() -> Self {
        Self {
            preference: UiScalePreference::Auto,
            revision: 0,
            preference_revision: 0,
            force_geometry_revision: 0,
        }
    }
}

/// Replaces any pending target; monitor events do not mark the preference as dirty.
pub(crate) fn request_scale_update(
    sender: &tokio::sync::watch::Sender<ScaleRequest>,
    preference: UiScalePreference,
    force_geometry: bool,
    preference_changed: bool,
) -> ScaleRequest {
    let mut updated = ScaleRequest::default();
    sender.send_modify(|request| {
        request.preference = preference;
        request.revision = request.revision.wrapping_add(1);
        if force_geometry {
            request.force_geometry_revision = request.force_geometry_revision.wrapping_add(1);
        }
        if preference_changed {
            request.preference_revision = request.preference_revision.wrapping_add(1);
        }
        updated = *request;
    });
    updated
}

/// Applies one target at a time and drains only the newest request after each transition.
pub(crate) async fn apply_latest_scale_request<F, Fut>(
    requests: &mut tokio::sync::watch::Receiver<ScaleRequest>,
    mut apply: F,
) -> Option<ScaleRequest>
where
    F: FnMut(ScaleRequest) -> Fut,
    Fut: Future<Output = ()>,
{
    loop {
        let request = *requests.borrow_and_update();
        apply(request).await;
        match requests.has_changed() {
            Ok(true) => {
                if requests.changed().await.is_err() {
                    return Some(request);
                }
            }
            Ok(false) | Err(_) => return Some(request),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct WindowBounds {
    width_logical: f64,
    height_logical: f64,
    width_inner_physical: u32,
    height_inner_physical: u32,
    min_width_logical: f64,
    min_height_logical: f64,
    x_physical: i32,
    y_physical: i32,
}

pub(crate) trait WindowBoundsTarget {
    fn current_monitor(&self) -> tauri::Result<Option<Monitor>>;
    fn primary_monitor(&self) -> tauri::Result<Option<Monitor>>;
    fn available_monitors(&self) -> tauri::Result<Vec<Monitor>>;
    fn set_min_size(&self, size: Size) -> tauri::Result<()>;
    fn set_size(&self, size: Size) -> tauri::Result<()>;
    fn set_position(&self, position: Position) -> tauri::Result<()>;
    fn outer_position(&self) -> tauri::Result<PhysicalPosition<i32>>;
    fn inner_size(&self) -> tauri::Result<PhysicalSize<u32>>;
    fn outer_size(&self) -> tauri::Result<PhysicalSize<u32>>;
}

impl WindowBoundsTarget for tauri::Window {
    fn current_monitor(&self) -> tauri::Result<Option<Monitor>> {
        tauri::Window::current_monitor(self)
    }

    fn primary_monitor(&self) -> tauri::Result<Option<Monitor>> {
        tauri::Window::primary_monitor(self)
    }

    fn available_monitors(&self) -> tauri::Result<Vec<Monitor>> {
        tauri::Window::available_monitors(self)
    }

    fn set_min_size(&self, size: Size) -> tauri::Result<()> {
        tauri::Window::set_min_size(self, Some(size))
    }

    fn set_size(&self, size: Size) -> tauri::Result<()> {
        tauri::Window::set_size(self, size)
    }

    fn set_position(&self, position: Position) -> tauri::Result<()> {
        tauri::Window::set_position(self, position)
    }

    fn outer_position(&self) -> tauri::Result<PhysicalPosition<i32>> {
        tauri::Window::outer_position(self)
    }

    fn inner_size(&self) -> tauri::Result<PhysicalSize<u32>> {
        tauri::Window::inner_size(self)
    }

    fn outer_size(&self) -> tauri::Result<PhysicalSize<u32>> {
        tauri::Window::outer_size(self)
    }
}

impl WindowBoundsTarget for tauri::WebviewWindow {
    fn current_monitor(&self) -> tauri::Result<Option<Monitor>> {
        tauri::WebviewWindow::current_monitor(self)
    }

    fn primary_monitor(&self) -> tauri::Result<Option<Monitor>> {
        tauri::WebviewWindow::primary_monitor(self)
    }

    fn available_monitors(&self) -> tauri::Result<Vec<Monitor>> {
        tauri::WebviewWindow::available_monitors(self)
    }

    fn set_min_size(&self, size: Size) -> tauri::Result<()> {
        tauri::WebviewWindow::set_min_size(self, Some(size))
    }

    fn set_size(&self, size: Size) -> tauri::Result<()> {
        tauri::WebviewWindow::set_size(self, size)
    }

    fn set_position(&self, position: Position) -> tauri::Result<()> {
        tauri::WebviewWindow::set_position(self, position)
    }

    fn outer_position(&self) -> tauri::Result<PhysicalPosition<i32>> {
        tauri::WebviewWindow::outer_position(self)
    }

    fn inner_size(&self) -> tauri::Result<PhysicalSize<u32>> {
        tauri::WebviewWindow::inner_size(self)
    }

    fn outer_size(&self) -> tauri::Result<PhysicalSize<u32>> {
        tauri::WebviewWindow::outer_size(self)
    }
}

fn valid_scale_factor(scale_factor: f64) -> f64 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

/// User-facing scale choice. Auto fits the largest preset window into the logical work area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum UiScalePreference {
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "100")]
    Percent100,
    #[serde(rename = "90")]
    Percent90,
    #[serde(rename = "80")]
    Percent80,
    #[serde(rename = "75")]
    Percent75,
}

/// Returns logical work-area dimensions, converting monitor physical pixels by its DPI scale.
pub(crate) fn logical_work_area_size(
    work_area: (i32, i32, u32, u32),
    scale_factor: f64,
) -> (f64, f64) {
    let scale_factor = valid_scale_factor(scale_factor);
    (
        work_area.2 as f64 / scale_factor,
        work_area.3 as f64 / scale_factor,
    )
}

fn minimum_inner_size_for_work_area(
    work_area: (i32, i32, u32, u32),
    scale_factor: f64,
    frame_size: PhysicalSize<u32>,
) -> LogicalSize<f64> {
    let (max_width, max_height) = logical_work_area_size(work_area, scale_factor);
    let max_width =
        (max_width - f64::from(frame_size.width) / valid_scale_factor(scale_factor)).max(1.0);
    let max_height =
        (max_height - f64::from(frame_size.height) / valid_scale_factor(scale_factor)).max(1.0);
    LogicalSize::new(
        MIN_WIDTH_LOGICAL.min(max_width).max(1.0),
        MIN_HEIGHT_LOGICAL.min(max_height).max(1.0),
    )
}

/// Returns the configured desktop window's inner size at a given zoom preset.
pub(crate) fn scaled_window_size(scale: f64) -> LogicalSize<f64> {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    LogicalSize::new(BASE_WIDTH_LOGICAL * scale, BASE_HEIGHT_LOGICAL * scale)
}

fn position_preserving_outer_center(
    current_position: PhysicalPosition<i32>,
    current_outer_size: PhysicalSize<u32>,
    current_inner_size: PhysicalSize<u32>,
    desired_inner_size: LogicalSize<f64>,
    scale_factor: f64,
) -> PhysicalPosition<i32> {
    let scale_factor = valid_scale_factor(scale_factor);
    let frame_width = current_outer_size
        .width
        .saturating_sub(current_inner_size.width);
    let frame_height = current_outer_size
        .height
        .saturating_sub(current_inner_size.height);
    let desired_outer_width =
        (desired_inner_size.width * scale_factor).round() as f64 + f64::from(frame_width);
    let desired_outer_height =
        (desired_inner_size.height * scale_factor).round() as f64 + f64::from(frame_height);
    PhysicalPosition::new(
        (f64::from(current_position.x) + f64::from(current_outer_size.width) / 2.0
            - desired_outer_width / 2.0)
            .round() as i32,
        (f64::from(current_position.y) + f64::from(current_outer_size.height) / 2.0
            - desired_outer_height / 2.0)
            .round() as i32,
    )
}

/// Auto picks the largest preset whose base inner size fits with a small logical margin.
pub(crate) fn auto_scale_for_work_area(work_area: (i32, i32, u32, u32), scale_factor: f64) -> f64 {
    let (width, height) = logical_work_area_size(work_area, scale_factor);
    let comfortable_width = (width - AUTO_WORK_AREA_MARGIN_LOGICAL).max(0.0);
    let comfortable_height = (height - AUTO_WORK_AREA_MARGIN_LOGICAL).max(0.0);
    [1.0, 0.9, 0.8, 0.75]
        .into_iter()
        .find(|scale| {
            let size = scaled_window_size(*scale);
            size.width <= comfortable_width && size.height <= comfortable_height
        })
        .unwrap_or(0.75)
}

/// Resolves the zoom factor for a preference and a monitor's usable physical work area.
pub(crate) fn resolve_webview_scale(
    preference: UiScalePreference,
    work_area: (i32, i32, u32, u32),
    scale_factor: f64,
) -> f64 {
    match preference {
        UiScalePreference::Percent100 => 1.0,
        UiScalePreference::Percent90 => 0.9,
        UiScalePreference::Percent80 => 0.8,
        UiScalePreference::Percent75 => 0.75,
        UiScalePreference::Auto => auto_scale_for_work_area(work_area, scale_factor),
    }
}

/// Resolves a preference directly from monitor metadata for setup and monitor-change handlers.
pub(crate) fn resolve_webview_scale_for_monitor(
    preference: UiScalePreference,
    monitor: &Monitor,
) -> f64 {
    resolve_webview_scale(preference, work_area(monitor), monitor.scale_factor())
}

/// Avoids redundant native zoom calls when the resolved factor has not materially changed.
pub(crate) fn should_update_webview_scale(previous: Option<f64>, next: f64) -> bool {
    if !next.is_finite() || next <= 0.0 {
        return false;
    }

    previous.is_none_or(|previous| {
        !previous.is_finite() || (previous - next).abs() > ZOOM_CHANGE_EPSILON
    })
}

pub(crate) trait WebviewScaleTarget {
    fn set_zoom(&self, scale_factor: f64) -> tauri::Result<()>;
}

impl WebviewScaleTarget for tauri::WebviewWindow {
    fn set_zoom(&self, scale_factor: f64) -> tauri::Result<()> {
        tauri::WebviewWindow::set_zoom(self, scale_factor)
    }
}

/// Applies the zoom preset and matching physical window size while preserving its current center.
/// Window sizing uses Tauri's logical inner size; placement and work-area clamping use physical
/// outer bounds. The resize is issued before zoom so the final viewport is ready for the new scale.
pub(crate) fn apply_interface_scale(
    webview: &(impl WindowBoundsTarget + WebviewScaleTarget),
    preference: UiScalePreference,
    last_applied: &mut Option<f64>,
    force_resize: bool,
) -> tauri::Result<bool> {
    let Some(monitor) = monitor_for_window(webview) else {
        return Ok(false);
    };
    let scale = resolve_webview_scale_for_monitor(preference, &monitor);
    let scale_changed = should_update_webview_scale(*last_applied, scale);
    if !scale_changed && !force_resize {
        return Ok(false);
    }

    let work_area = work_area(&monitor);
    let monitor_scale = valid_scale_factor(monitor.scale_factor());
    let target_size = scaled_window_size(scale);
    let current_position = webview.outer_position().ok();
    let current_outer = webview.outer_size().ok();
    let current_inner = webview.inner_size().ok();
    let desired_position = current_position
        .zip(current_outer)
        .map(|(position, outer)| {
            position_preserving_outer_center(
                position,
                outer,
                current_inner.unwrap_or(outer),
                target_size,
                monitor_scale,
            )
        });
    let frame_size = current_outer
        .zip(current_inner)
        .map(|(outer, inner)| {
            PhysicalSize::new(
                outer.width.saturating_sub(inner.width),
                outer.height.saturating_sub(inner.height),
            )
        })
        .unwrap_or_else(|| PhysicalSize::new(0, 0));
    let Some(bounds) = calculate_window_bounds_with_frame(
        target_size.width,
        target_size.height,
        desired_position.map(|position| (position.x, position.y)),
        work_area,
        monitor_scale,
        frame_size,
        false,
    ) else {
        return Ok(false);
    };

    apply_bounds(webview, bounds)?;
    if scale_changed {
        timed_window_api("set_zoom", || webview.set_zoom(scale))?;
        *last_applied = Some(scale);
    }
    Ok(true)
}

/// Resolves the current monitor and applies the selected interface scale if it changed.
/// Invoke after loading the persisted preference and whenever the user changes it.
pub(crate) fn apply_current_interface_scale(
    webview: &(impl WindowBoundsTarget + WebviewScaleTarget),
    preference: UiScalePreference,
    last_applied: &mut Option<f64>,
) -> tauri::Result<bool> {
    apply_interface_scale(webview, preference, last_applied, true)
}

#[cfg(test)]
fn calculate_window_bounds(
    desired_width_logical: f64,
    desired_height_logical: f64,
    desired_position_physical: Option<(i32, i32)>,
    work_area: (i32, i32, u32, u32),
    scale_factor: f64,
    center: bool,
) -> Option<WindowBounds> {
    calculate_window_bounds_with_frame(
        desired_width_logical,
        desired_height_logical,
        desired_position_physical,
        work_area,
        scale_factor,
        PhysicalSize::new(0, 0),
        center,
    )
}

fn calculate_window_bounds_with_frame(
    desired_width_logical: f64,
    desired_height_logical: f64,
    desired_position_physical: Option<(i32, i32)>,
    work_area: (i32, i32, u32, u32),
    scale_factor: f64,
    frame_size: PhysicalSize<u32>,
    center: bool,
) -> Option<WindowBounds> {
    let (work_x, work_y, work_width, work_height) = work_area;
    if work_width == 0 || work_height == 0 {
        return None;
    }

    let scale_factor = valid_scale_factor(scale_factor);
    let max_inner_width_physical = work_width.saturating_sub(frame_size.width);
    let max_inner_height_physical = work_height.saturating_sub(frame_size.height);
    let max_width_logical = max_inner_width_physical as f64 * WORK_AREA_FRACTION / scale_factor;
    let max_height_logical = max_inner_height_physical as f64 * WORK_AREA_FRACTION / scale_factor;
    // Keep the resize floor useful for the UI, but never require more space than this
    // monitor's work area can provide at its current DPI scale.
    let min_size = minimum_inner_size_for_work_area(work_area, scale_factor, frame_size);
    let min_width_logical = min_size.width;
    let min_height_logical = min_size.height;
    let width_logical = safe_desired_size(desired_width_logical, min_width_logical)
        .clamp(min_width_logical, max_width_logical.max(min_width_logical));
    let height_logical = safe_desired_size(desired_height_logical, min_height_logical).clamp(
        min_height_logical,
        max_height_logical.max(min_height_logical),
    );

    let inner_width_physical = (width_logical * scale_factor)
        .round()
        .clamp(1.0, max_inner_width_physical.max(1) as f64) as u32;
    let inner_height_physical = (height_logical * scale_factor)
        .round()
        .clamp(1.0, max_inner_height_physical.max(1) as f64) as u32;
    let width_physical = inner_width_physical.saturating_add(frame_size.width);
    let height_physical = inner_height_physical.saturating_add(frame_size.height);
    let max_x = work_x.saturating_add(work_width.saturating_sub(width_physical) as i32);
    let max_y = work_y.saturating_add(work_height.saturating_sub(height_physical) as i32);

    let (x_physical, y_physical) = if center {
        (
            work_x.saturating_add((work_width - width_physical) as i32 / 2),
            work_y.saturating_add((work_height - height_physical) as i32 / 2),
        )
    } else {
        let (desired_x, desired_y) = desired_position_physical.unwrap_or((work_x, work_y));
        (
            desired_x.clamp(work_x, max_x),
            desired_y.clamp(work_y, max_y),
        )
    };

    Some(WindowBounds {
        width_logical,
        height_logical,
        width_inner_physical: inner_width_physical,
        height_inner_physical: inner_height_physical,
        min_width_logical,
        min_height_logical,
        x_physical,
        y_physical,
    })
}

fn safe_desired_size(desired: f64, fallback: f64) -> f64 {
    if desired.is_finite() && desired > 0.0 {
        desired
    } else {
        fallback
    }
}

fn work_area(monitor: &Monitor) -> (i32, i32, u32, u32) {
    let rect = monitor.work_area();
    (
        rect.position.x,
        rect.position.y,
        rect.size.width,
        rect.size.height,
    )
}

fn monitor_for_window(window: &impl WindowBoundsTarget) -> Option<Monitor> {
    timed_window_api("current_monitor", || window.current_monitor())
        .ok()
        .flatten()
        .or_else(|| {
            timed_window_api("primary_monitor", || window.primary_monitor())
                .ok()
                .flatten()
        })
        .or_else(|| {
            timed_window_api("available_monitors", || window.available_monitors())
                .ok()?
                .into_iter()
                .next()
        })
}

fn apply_bounds(window: &impl WindowBoundsTarget, bounds: WindowBounds) -> tauri::Result<()> {
    timed_window_api("set_min_size", || {
        window.set_min_size(Size::Logical(LogicalSize::new(
            bounds.min_width_logical,
            bounds.min_height_logical,
        )))
    })?;
    let current_size = window.inner_size().ok();
    if current_size.is_none_or(|size| {
        size.width != bounds.width_inner_physical || size.height != bounds.height_inner_physical
    }) {
        timed_window_api("set_size", || {
            window.set_size(Size::Logical(LogicalSize::new(
                bounds.width_logical,
                bounds.height_logical,
            )))
        })?;
    }
    let current_position = window.outer_position().ok();
    if current_position
        .is_none_or(|position| position.x != bounds.x_physical || position.y != bounds.y_physical)
    {
        timed_window_api("set_position", || {
            window.set_position(Position::Physical(PhysicalPosition::new(
                bounds.x_physical,
                bounds.y_physical,
            )))
        })?;
    }
    Ok(())
}

fn timed_window_api<T>(
    operation: &'static str,
    call: impl FnOnce() -> tauri::Result<T>,
) -> tauri::Result<T> {
    let started = std::time::Instant::now();
    tracing::info!(
        operation,
        phase = "start",
        thread = ?std::thread::current().id(),
        "interface scale native operation"
    );
    let result = call();
    tracing::info!(
        operation,
        phase = "finish",
        success = result.is_ok(),
        elapsed_ms = started.elapsed().as_millis(),
        thread = ?std::thread::current().id(),
        "interface scale native operation"
    );
    result
}

/// Keeps the initial app window within the usable area of the monitor it opens on.
pub(crate) fn fit_initial_window(window: &impl WindowBoundsTarget) {
    let Some(monitor) = monitor_for_window(window) else {
        tracing::warn!("no monitor information available; keeping configured main window bounds");
        return;
    };

    let frame_size = window
        .outer_size()
        .ok()
        .zip(window.inner_size().ok())
        .map(|(outer, inner)| {
            PhysicalSize::new(
                outer.width.saturating_sub(inner.width),
                outer.height.saturating_sub(inner.height),
            )
        })
        .unwrap_or_else(|| PhysicalSize::new(0, 0));
    let Some(bounds) = calculate_window_bounds_with_frame(
        BASE_WIDTH_LOGICAL,
        BASE_HEIGHT_LOGICAL,
        None,
        work_area(&monitor),
        monitor.scale_factor(),
        frame_size,
        true,
    ) else {
        tracing::warn!("monitor work area is empty; keeping configured main window bounds");
        return;
    };

    if let Err(error) = apply_bounds(window, bounds) {
        tracing::warn!(%error, "failed to fit initial main window bounds");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn initial_size_is_preserved_when_it_fits_and_centered_in_work_area() {
        let bounds =
            calculate_window_bounds(1440.0, 938.0, None, (0, 40, 2560, 1400), 1.0, true).unwrap();

        assert_eq!(bounds.width_logical, 1440.0);
        assert_eq!(bounds.height_logical, 938.0);
        assert_eq!((bounds.x_physical, bounds.y_physical), (560, 271));
    }

    #[test]
    fn notebook_work_area_clamps_logical_size_at_125_percent_dpi() {
        let bounds =
            calculate_window_bounds(1440.0, 938.0, None, (0, 0, 1366, 728), 1.25, true).unwrap();

        assert_eq!(bounds.width_logical, 1366.0 / 1.25);
        assert_eq!(bounds.height_logical, 728.0 / 1.25);
        assert_eq!((bounds.x_physical, bounds.y_physical), (0, 0));
    }

    #[test]
    fn secondary_monitor_with_negative_origin_stays_inside_its_work_area() {
        let bounds = calculate_window_bounds(
            1200.0,
            800.0,
            Some((-2400, -100)),
            (-1920, 0, 1920, 1040),
            1.0,
            false,
        )
        .unwrap();

        assert_eq!((bounds.x_physical, bounds.y_physical), (-1920, 0));
        assert!(bounds.x_physical >= -1920);
        assert!(bounds.y_physical >= 0);
        assert!(bounds.x_physical + (bounds.width_logical * 1.0) as i32 <= 0);
        assert!(bounds.y_physical + (bounds.height_logical * 1.0) as i32 <= 1040);
    }

    #[test]
    fn invalid_scale_factor_falls_back_to_one_and_empty_area_is_rejected() {
        let bounds =
            calculate_window_bounds(500.0, 400.0, None, (0, 0, 800, 600), f64::NAN, true).unwrap();
        assert_eq!(
            (bounds.width_logical, bounds.height_logical),
            (800.0, 600.0)
        );
        assert!(calculate_window_bounds(500.0, 400.0, None, (0, 0, 0, 600), 1.0, true).is_none());
    }

    #[test]
    fn minimum_window_size_is_clamped_to_the_logical_work_area() {
        let bounds =
            calculate_window_bounds(500.0, 400.0, None, (0, 0, 900, 500), 1.25, true).unwrap();

        assert_eq!(bounds.min_width_logical, 720.0);
        assert_eq!(bounds.min_height_logical, 400.0);
        assert_eq!(bounds.width_logical, 720.0);
        assert_eq!(bounds.height_logical, 400.0);
    }

    #[test]
    fn invalid_requested_window_size_uses_the_safe_minimum() {
        let bounds =
            calculate_window_bounds(f64::NAN, f64::INFINITY, None, (0, 0, 1920, 1080), 1.0, true)
                .unwrap();

        assert_eq!(bounds.width_logical, MIN_WIDTH_LOGICAL);
        assert_eq!(bounds.height_logical, MIN_HEIGHT_LOGICAL);
    }

    #[test]
    fn auto_scale_picks_largest_fitting_preset_after_dpi_and_safety_margin() {
        assert_eq!(
            resolve_webview_scale(UiScalePreference::Auto, (0, 0, 1920, 1040), 1.0),
            1.0
        );
        assert_eq!(
            resolve_webview_scale(UiScalePreference::Auto, (0, 0, 1920, 1040), 1.25),
            0.8
        );
        assert_eq!(
            resolve_webview_scale(UiScalePreference::Auto, (0, 0, 1380, 720), 1.0),
            0.75
        );
        // Main notebook case: 1366×768 at 100% has a 1366×728 logical work area.
        assert_eq!(
            resolve_webview_scale(UiScalePreference::Auto, (0, 0, 1366, 728), 1.0),
            0.75
        );
        // 1920×1080 at 150% is 1280×693 logical, so it is smaller than 100% mode.
        assert_eq!(
            resolve_webview_scale(UiScalePreference::Auto, (0, 0, 1920, 1040), 1.5),
            0.75
        );
        assert_eq!(
            logical_work_area_size((0, 0, 1366, 728), 1.25),
            (1092.8, 582.4)
        );
    }

    #[test]
    fn scaled_window_sizes_match_the_configured_base_for_each_preset() {
        for (scale, expected_width, expected_height) in [
            (1.0, 1440.0, 938.0),
            (0.9, 1296.0, 844.2),
            (0.8, 1152.0, 750.4),
            (0.75, 1080.0, 703.5),
        ] {
            let size = scaled_window_size(scale);
            assert!((size.width - expected_width).abs() < f64::EPSILON);
            assert!((size.height - expected_height).abs() < 1e-9);
        }
    }

    #[test]
    fn small_monitor_clamps_the_scaled_window_inside_its_work_area() {
        let bounds = calculate_window_bounds(
            scaled_window_size(0.75).width,
            scaled_window_size(0.75).height,
            None,
            (0, 0, 1000, 650),
            1.0,
            true,
        )
        .unwrap();

        assert_eq!(
            (bounds.width_logical, bounds.height_logical),
            (1000.0, 650.0)
        );
        assert_eq!((bounds.x_physical, bounds.y_physical), (0, 0));
    }

    #[test]
    fn scaled_window_clamp_converts_work_area_through_dpi() {
        let size = scaled_window_size(0.75);
        let bounds =
            calculate_window_bounds(size.width, size.height, None, (0, 0, 1920, 1040), 1.5, true)
                .unwrap();

        assert_eq!(bounds.width_logical, 1080.0);
        assert_eq!(bounds.height_logical, 1040.0 / 1.5);
        assert_eq!((bounds.x_physical, bounds.y_physical), (150, 0));
    }

    #[test]
    fn manual_scale_preserves_the_outer_window_center_before_work_area_clamp() {
        let position = position_preserving_outer_center(
            PhysicalPosition::new(100, 80),
            PhysicalSize::new(1450, 948),
            PhysicalSize::new(1440, 938),
            scaled_window_size(0.75),
            1.0,
        );

        assert_eq!((position.x, position.y), (280, 197));
    }

    #[test]
    fn explicit_scale_choices_are_stable_and_invalid_dpi_falls_back_safely() {
        assert_eq!(
            resolve_webview_scale(UiScalePreference::Percent100, (0, 0, 400, 300), 1.5),
            1.0
        );
        assert_eq!(
            resolve_webview_scale(UiScalePreference::Percent90, (0, 0, 400, 300), 1.5),
            0.9
        );
        assert_eq!(
            resolve_webview_scale(UiScalePreference::Percent80, (0, 0, 400, 300), 1.5),
            0.8
        );
        assert_eq!(
            resolve_webview_scale(UiScalePreference::Percent75, (0, 0, 400, 300), 1.5),
            0.75
        );
        assert_eq!(
            resolve_webview_scale(UiScalePreference::Auto, (0, 0, 1600, 900), f64::NAN),
            0.9
        );
    }

    #[test]
    fn scale_update_guard_skips_unchanged_and_rejects_invalid_factors() {
        assert!(should_update_webview_scale(None, 0.9));
        assert!(!should_update_webview_scale(Some(0.9), 0.9));
        assert!(!should_update_webview_scale(Some(0.9), 0.9005));
        assert!(should_update_webview_scale(Some(0.9), 0.91));
        assert!(!should_update_webview_scale(None, f64::NAN));
        assert!(!should_update_webview_scale(None, 0.0));
    }

    #[test]
    fn scale_preference_serializes_to_stable_setting_values() {
        assert_eq!(
            serde_json::to_string(&UiScalePreference::Auto).unwrap(),
            "\"auto\""
        );
        assert_eq!(
            serde_json::to_string(&UiScalePreference::Percent100).unwrap(),
            "\"100\""
        );
        assert_eq!(
            serde_json::to_string(&UiScalePreference::Percent90).unwrap(),
            "\"90\""
        );
        assert_eq!(
            serde_json::to_string(&UiScalePreference::Percent80).unwrap(),
            "\"80\""
        );
        assert_eq!(
            serde_json::to_string(&UiScalePreference::Percent75).unwrap(),
            "\"75\""
        );
        assert_eq!(
            serde_json::from_str::<UiScalePreference>("\"auto\"").unwrap(),
            UiScalePreference::Auto
        );
        assert_eq!(UiScalePreference::default(), UiScalePreference::Auto);
    }

    #[tokio::test]
    async fn rapid_scale_requests_apply_only_the_latest_pending_target() {
        use UiScalePreference as Scale;

        let (sender, mut receiver) = tokio::sync::watch::channel(ScaleRequest::default());
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        let worker = tokio::spawn(async move {
            let mut applied = Vec::new();
            let mut block_first_transition = true;
            while receiver.changed().await.is_ok() {
                let entered = worker_entered.clone();
                let release = worker_release.clone();
                let mut pause_first = std::mem::take(&mut block_first_transition);
                let _ = apply_latest_scale_request(&mut receiver, |request| {
                    applied.push(request);
                    let pause = std::mem::take(&mut pause_first);
                    let entered = entered.clone();
                    let release = release.clone();
                    async move {
                        if pause {
                            entered.notify_one();
                            release.notified().await;
                        }
                    }
                })
                .await;
            }
            applied
        });

        request_scale_update(&sender, Scale::Percent100, false, true);
        tokio::time::timeout(std::time::Duration::from_secs(2), entered.notified())
            .await
            .expect("worker should begin first transition");

        let sequence = [
            Scale::Percent100,
            Scale::Percent75,
            Scale::Percent100,
            Scale::Percent75,
            Scale::Percent90,
            Scale::Percent80,
            Scale::Percent75,
            Scale::Percent100,
            Scale::Percent75,
        ];
        let mut final_request = ScaleRequest::default();
        for index in 0..99 {
            final_request =
                request_scale_update(&sender, sequence[index % sequence.len()], false, true);
        }
        assert_eq!(final_request.revision, 100);
        assert_eq!(final_request.preference, Scale::Percent75);

        release.notify_one();
        drop(sender);
        let applied = tokio::time::timeout(std::time::Duration::from_secs(2), worker)
            .await
            .expect("scale request worker should not deadlock")
            .unwrap();
        assert_eq!(applied.len(), 2, "intermediate targets should be coalesced");
        assert_eq!(applied.last().unwrap().preference, Scale::Percent75);
        assert_eq!(applied.last().unwrap().revision, 100);
    }
}
