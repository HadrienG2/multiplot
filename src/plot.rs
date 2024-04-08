//! Where traces get drawn into a plot

use crate::{criterion::ThroughputType, trace::Traces, Args, Result};
use anyhow::Context;
use colorous::SINEBOW;
use plotters::{backend::RGBPixel, prelude::*};
use plotters_backend::{
    BackendColor, BackendCoord, BackendStyle, BackendTextStyle, DrawingErrorKind,
};
use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    ops::{Deref, DerefMut},
    path::Path,
};

/// Draw the plot
pub fn draw(args: &Args, traces: Traces) -> Result<()> {
    // Set up the drawing area
    let root = DrawingBackendImpl::new(&args.output_path, args.plot_size())
        .context("setting up the plot's drawing area")?
        .into_drawing_area();
    root.fill(&WHITE)
        .context("filling the plot's drawing area")?;

    // Determine the plotting range
    let (x_range, y_range) = traces.xy_range();

    // Set up the chart
    let mut chart = ChartBuilder::on(&root);
    if !args.title.is_empty() {
        chart.caption(&args.title, ("sans-serif", 5.percent_height()));
    }
    let mut chart = chart
        .set_label_area_size(LabelAreaPosition::Left, 12.percent_width())
        .set_label_area_size(LabelAreaPosition::Bottom, 5.percent_height())
        .margin(1.percent())
        .build_cartesian_2d(
            x_range.log_scale(),
            y_range.log_scale().with_key_points(vec![
                2.0e8, 5.0e8, 1.0e9, 2.0e9, 5.0e9, 1.0e10, 2.0e10, 5.0e10,
            ]),
        )
        .context("setting up the plot's chart")?;

    // Set up the mesh
    chart
        .configure_mesh()
        .x_desc(args.x_label.to_string())
        .y_desc(match traces.throughput {
            None => "s".to_string(),
            Some(ThroughputType::Bytes) | Some(ThroughputType::BytesDecimal) => "B/s".to_string(),
            Some(ThroughputType::Elements) => format!("{}/s", args.element_throughput_unit),
        })
        .label_style(("sans-serif", 2.percent_height()))
        .draw()
        .context("setting up the plot's mesh")?;

    // Draw the traces
    let num_traces = traces.len();
    let color_pos_norm = 1.0 / num_traces as f64;
    for (idx, trace) in traces.per_trace_data.into_vec().into_iter().enumerate() {
        // Pick the trace color
        let color_pos = idx as f64 * color_pos_norm;
        let color = SINEBOW.eval_continuous(color_pos);
        let color = RGBColor(color.r, color.g, color.b);

        // Draw the trace
        chart
            .draw_series(LineSeries::new(
                trace
                    .data
                    .iter()
                    .map(|(x, meas)| (*x as f64, meas.point_estimate)),
                color,
            ))
            .with_context(|| format!("drawing trace {}", trace.name))?
            .label(trace.name)
            .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], color));

        // Draw the error bars
        chart.draw_series(trace.data.iter().map(|(x, meas)| {
            ErrorBar::new_vertical(
                *x as f64,
                meas.lower_bound,
                meas.point_estimate,
                meas.upper_bound,
                color,
                (0.008 * args.height.get() as f32) as u32,
            )
        }))?;
    }

    // Draw the legend
    chart
        .configure_series_labels()
        .border_style(BLACK)
        .background_style(WHITE.filled())
        .position(SeriesLabelPosition::LowerRight)
        .label_font({
            let ideal_size_percent = 2.25f64;
            let max_size_percent = 50.0 / num_traces as f64;
            (
                "sans-serif",
                (ideal_size_percent.min(max_size_percent)).percent_height(),
            )
        })
        .draw()
        .context("drawing the legend")?;

    // Manually call preset to avoid errors being silently ignored
    root.present()
        .context("failed to write the plot to the output file")
}

/// Abstraction over the multiple DrawingBackends provided by plotters
///
/// `dyn DrawingBackend` is not applicable here as the trait is not object-safe.
enum DrawingBackendImpl<'path> {
    /// Bitmap drawing backend
    Bitmap(BitMapBackend<'path, RGBPixel>),

    /// SVG drawing backend
    Svg(SVGBackend<'path>),
}
//
impl<'path> DrawingBackendImpl<'path> {
    /// Pick drawing backend based on file extension
    pub fn new(path: &'path impl AsRef<Path>, wh: (u32, u32)) -> Result<Self> {
        let path = path.as_ref();
        let extension = path
            .extension()
            .context("need file extension to pick backend")?;
        if extension == "svg" {
            Ok(Self::svg(path, wh))
        } else {
            Ok(Self::bitmap(path, wh))
        }
    }

    /// Create a bitmap drawing backend
    pub fn bitmap(path: &'path (impl AsRef<Path> + ?Sized), wh: (u32, u32)) -> Self {
        Self::Bitmap(BitMapBackend::new(path, wh))
    }

    /// Create an SVG drawing backend
    pub fn svg(path: &'path (impl AsRef<Path> + ?Sized), wh: (u32, u32)) -> Self {
        Self::Svg(SVGBackend::new(path, wh))
    }
}
//
impl DrawingBackend for DrawingBackendImpl<'_> {
    type ErrorType = AnyhowError;

    fn get_size(&self) -> (u32, u32) {
        match self {
            Self::Bitmap(b) => b.get_size(),
            Self::Svg(s) => s.get_size(),
        }
    }

    fn ensure_prepared(&mut self) -> std::result::Result<(), DrawingErrorKind<Self::ErrorType>> {
        match self {
            Self::Bitmap(b) => b
                .ensure_prepared()
                .map_err(AnyhowError::erase_drawing_error_kind),
            Self::Svg(s) => s
                .ensure_prepared()
                .map_err(AnyhowError::erase_drawing_error_kind),
        }
    }

    fn present(&mut self) -> std::result::Result<(), DrawingErrorKind<Self::ErrorType>> {
        match self {
            Self::Bitmap(b) => b.present().map_err(AnyhowError::erase_drawing_error_kind),
            Self::Svg(s) => s.present().map_err(AnyhowError::erase_drawing_error_kind),
        }
    }

    fn draw_pixel(
        &mut self,
        point: BackendCoord,
        color: BackendColor,
    ) -> std::result::Result<(), DrawingErrorKind<Self::ErrorType>> {
        match self {
            Self::Bitmap(b) => b
                .draw_pixel(point, color)
                .map_err(AnyhowError::erase_drawing_error_kind),
            Self::Svg(s) => s
                .draw_pixel(point, color)
                .map_err(AnyhowError::erase_drawing_error_kind),
        }
    }

    fn draw_line<S: BackendStyle>(
        &mut self,
        from: BackendCoord,
        to: BackendCoord,
        style: &S,
    ) -> std::result::Result<(), DrawingErrorKind<Self::ErrorType>> {
        match self {
            Self::Bitmap(b) => b
                .draw_line(from, to, style)
                .map_err(AnyhowError::erase_drawing_error_kind),
            Self::Svg(s) => s
                .draw_line(from, to, style)
                .map_err(AnyhowError::erase_drawing_error_kind),
        }
    }

    fn draw_rect<S: BackendStyle>(
        &mut self,
        upper_left: BackendCoord,
        bottom_right: BackendCoord,
        style: &S,
        fill: bool,
    ) -> std::result::Result<(), DrawingErrorKind<Self::ErrorType>> {
        match self {
            Self::Bitmap(b) => b
                .draw_rect(upper_left, bottom_right, style, fill)
                .map_err(AnyhowError::erase_drawing_error_kind),
            Self::Svg(s) => s
                .draw_rect(upper_left, bottom_right, style, fill)
                .map_err(AnyhowError::erase_drawing_error_kind),
        }
    }

    fn draw_path<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        path: I,
        style: &S,
    ) -> std::result::Result<(), DrawingErrorKind<Self::ErrorType>> {
        match self {
            Self::Bitmap(b) => b
                .draw_path(path, style)
                .map_err(AnyhowError::erase_drawing_error_kind),
            Self::Svg(s) => s
                .draw_path(path, style)
                .map_err(AnyhowError::erase_drawing_error_kind),
        }
    }

    fn draw_circle<S: BackendStyle>(
        &mut self,
        center: BackendCoord,
        radius: u32,
        style: &S,
        fill: bool,
    ) -> std::result::Result<(), DrawingErrorKind<Self::ErrorType>> {
        match self {
            Self::Bitmap(b) => b
                .draw_circle(center, radius, style, fill)
                .map_err(AnyhowError::erase_drawing_error_kind),
            Self::Svg(s) => s
                .draw_circle(center, radius, style, fill)
                .map_err(AnyhowError::erase_drawing_error_kind),
        }
    }

    fn fill_polygon<S: BackendStyle, I: IntoIterator<Item = BackendCoord>>(
        &mut self,
        vert: I,
        style: &S,
    ) -> std::result::Result<(), DrawingErrorKind<Self::ErrorType>> {
        match self {
            Self::Bitmap(b) => b
                .fill_polygon(vert, style)
                .map_err(AnyhowError::erase_drawing_error_kind),
            Self::Svg(s) => s
                .fill_polygon(vert, style)
                .map_err(AnyhowError::erase_drawing_error_kind),
        }
    }

    fn draw_text<TStyle: BackendTextStyle>(
        &mut self,
        text: &str,
        style: &TStyle,
        pos: BackendCoord,
    ) -> std::result::Result<(), DrawingErrorKind<Self::ErrorType>> {
        match self {
            Self::Bitmap(b) => b
                .draw_text(text, style, pos)
                .map_err(AnyhowError::erase_drawing_error_kind),
            Self::Svg(s) => s
                .draw_text(text, style, pos)
                .map_err(AnyhowError::erase_drawing_error_kind),
        }
    }

    fn estimate_text_size<TStyle: BackendTextStyle>(
        &self,
        text: &str,
        style: &TStyle,
    ) -> std::result::Result<(u32, u32), DrawingErrorKind<Self::ErrorType>> {
        match self {
            Self::Bitmap(b) => b
                .estimate_text_size(text, style)
                .map_err(AnyhowError::erase_drawing_error_kind),
            Self::Svg(s) => s
                .estimate_text_size(text, style)
                .map_err(AnyhowError::erase_drawing_error_kind),
        }
    }

    fn blit_bitmap(
        &mut self,
        pos: BackendCoord,
        iwh: (u32, u32),
        src: &[u8],
    ) -> std::result::Result<(), DrawingErrorKind<Self::ErrorType>> {
        match self {
            Self::Bitmap(b) => b
                .blit_bitmap(pos, iwh, src)
                .map_err(AnyhowError::erase_drawing_error_kind),
            Self::Svg(s) => s
                .blit_bitmap(pos, iwh, src)
                .map_err(AnyhowError::erase_drawing_error_kind),
        }
    }
}

/// [`anyhow::Error`] wrapper that implements [`std::error::Error`]
#[derive(Debug)]
struct AnyhowError(anyhow::Error);
//
impl AnyhowError {
    /// The From impl that we can't implement without dropping the Error impl...
    #[allow(unused)]
    pub fn new(error: impl Error + Send + Sync + 'static) -> Self {
        Self(error.into())
    }

    /// Wrap a plotters DrawingErrorKind
    pub fn erase_drawing_error_kind(
        ek: DrawingErrorKind<impl Error + Send + Sync + 'static>,
    ) -> DrawingErrorKind<Self> {
        match ek {
            DrawingErrorKind::DrawingError(e) => DrawingErrorKind::DrawingError(Self(e.into())),
            DrawingErrorKind::FontError(f) => DrawingErrorKind::FontError(f),
        }
    }

    /// Decay into a boxed standard error
    #[allow(unused)]
    pub fn into_boxed_error(self) -> Box<dyn Error + 'static> {
        self.0.into()
    }

    /// Decay into a boxed standard error, with Send bound
    #[allow(unused)]
    pub fn into_boxed_error_send(self) -> Box<dyn Error + Send + 'static> {
        self.0.into()
    }

    /// Decay into a boxed standard error, with Send and Sync bounds
    #[allow(unused)]
    pub fn into_boxed_error_sync(self) -> Box<dyn Error + Send + Sync + 'static> {
        self.0.into()
    }
}
//
impl AsRef<dyn Error> for AnyhowError {
    fn as_ref(&self) -> &(dyn Error + 'static) {
        self.0.as_ref()
    }
}
//
impl AsRef<dyn Error + Send + Sync> for AnyhowError {
    fn as_ref(&self) -> &(dyn Error + Send + Sync + 'static) {
        self.0.as_ref()
    }
}
//
impl Deref for AnyhowError {
    type Target = anyhow::Error;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
//
impl DerefMut for AnyhowError {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
//
impl Display for AnyhowError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        <anyhow::Error as Display>::fmt(&self.0, f)
    }
}
//
impl Error for AnyhowError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}
