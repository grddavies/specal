//! Minimal iced front end: drop an audio file on the window to see its spectrogram.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::alignment::Vertical;
use iced::futures::channel::oneshot;
use iced::widget::canvas::{self, Frame, Geometry, Stroke};
use iced::widget::{Canvas, center, container, image, stack, text};
use iced::{
  Border, Color, Element, Event, Fill, Font, Pixels, Point, Rectangle, Renderer, Size, Subscription, Task, Theme,
  event, font, mouse, window,
};
use specal_core::layout::{self, Layout};
use specal_core::{Analysis, DbRange, Palette, Settings, analyse_cancellable, ticks};

/// Rough glyph width as a fraction of font size, for spacing time labels.
const CHAR_WIDTH: f32 = 0.6;
const INITIAL_SIZE: Size = Size::new(1200.0, 700.0);

pub fn main() -> iced::Result {
  iced::application(App::boot, App::update, App::view)
    .title(App::title)
    .subscription(App::subscription)
    .theme(Theme::Dark)
    .window_size(INITIAL_SIZE)
    .run()
}

#[derive(Debug, Clone)]
enum Message {
  FileHovered,
  HoverLeft,
  FileDropped(PathBuf),
  Resized(Size),
  Analysed(u64, Result<Loaded, String>),
}

/// A finished analysis, ready to draw.
#[derive(Debug, Clone)]
struct Loaded {
  analysis: Arc<Analysis>,
  spectrogram: image::Handle,
}

struct App {
  settings: Settings,
  palette: Palette,
  range: DbRange,
  window_size: Size,
  hovering: bool,
  path: Option<PathBuf>,
  loaded: Option<Loaded>,
  analysing: bool,
  error: Option<String>,
  /// Bumped on every new analysis so stale results are ignored.
  generation: u64,
  cancel: Arc<AtomicBool>,
  /// Columns in the current (or in-flight) analysis, to spot resizes that need a new one.
  columns: usize,
  bar: image::Handle,
  cache: canvas::Cache,
}

impl App {
  fn boot() -> (Self, Task<Message>) {
    let palette = Palette::default();
    let bar = palette.bar(256);
    let mut app = Self {
      settings: Settings::default(),
      palette,
      range: DbRange::default(),
      window_size: INITIAL_SIZE,
      hovering: false,
      path: None,
      loaded: None,
      analysing: false,
      error: None,
      generation: 0,
      cancel: Arc::new(AtomicBool::new(false)),
      columns: 0,
      bar: image::Handle::from_rgba(bar.width, bar.height, bar.pixels),
      cache: canvas::Cache::new(),
    };
    // `specal-gui some-file.flac` opens it straight away.
    let task = match std::env::args_os().nth(1) {
      Some(path) => app.open(PathBuf::from(path)),
      None => Task::none(),
    };
    (app, task)
  }

  fn title(&self) -> String {
    match &self.path {
      Some(path) => format!("Specal – {}", file_name(path)),
      None => "Specal".to_owned(),
    }
  }

  fn subscription(&self) -> Subscription<Message> {
    event::listen_with(|event, _status, _window| match event {
      Event::Window(window::Event::FileHovered(_)) => Some(Message::FileHovered),
      Event::Window(window::Event::FilesHoveredLeft) => Some(Message::HoverLeft),
      Event::Window(window::Event::FileDropped(path)) => Some(Message::FileDropped(path)),
      Event::Window(window::Event::Resized(size)) => Some(Message::Resized(size)),
      _ => None,
    })
  }

  fn update(&mut self, message: Message) -> Task<Message> {
    match message {
      Message::FileHovered => {
        self.hovering = true;
        Task::none()
      }
      Message::HoverLeft => {
        self.hovering = false;
        Task::none()
      }
      Message::FileDropped(path) => {
        self.hovering = false;
        self.open(path)
      }
      Message::Resized(size) => {
        self.window_size = size;
        // Like Spek, re-analyse when the plot width changes so columns stay 1:1 with pixels.
        if self.path.is_some() && Layout::new(size.width, size.height).columns() != self.columns {
          self.start_analysis()
        } else {
          Task::none()
        }
      }
      Message::Analysed(generation, result) => {
        if generation == self.generation {
          self.analysing = false;
          match result {
            Ok(loaded) => {
              self.loaded = Some(loaded);
              self.error = None;
            }
            Err(e) => self.error = Some(e),
          }
          self.cache.clear();
        }
        Task::none()
      }
    }
  }

  fn open(&mut self, path: PathBuf) -> Task<Message> {
    self.path = Some(path);
    self.loaded = None;
    self.error = None;
    self.start_analysis()
  }

  /// Cancel any running analysis and start a new one on a background thread.
  fn start_analysis(&mut self) -> Task<Message> {
    let Some(path) = self.path.clone() else { return Task::none() };
    let columns = Layout::new(self.window_size.width, self.window_size.height).columns();
    if columns == 0 {
      return Task::none();
    }

    self.cancel.store(true, Ordering::Relaxed);
    let cancel = Arc::new(AtomicBool::new(false));
    self.cancel = cancel.clone();
    self.generation += 1;
    self.columns = columns;
    self.analysing = true;

    let generation = self.generation;
    let (settings, palette, range) = (self.settings.clone(), self.palette, self.range);
    Task::perform(
      async move {
        let (tx, rx) = oneshot::channel();
        std::thread::spawn(move || {
          let result = analyse_cancellable(&path, &settings, columns, &cancel).map(|analysis| {
            let rgba = analysis.spectrogram.to_rgba(palette, range);
            Loaded {
              analysis: Arc::new(analysis),
              spectrogram: image::Handle::from_rgba(rgba.width, rgba.height, rgba.pixels),
            }
          });
          let _ = tx.send(result.map_err(|e| e.to_string()));
        });
        rx.await.unwrap_or_else(|_| Err("analysis thread stopped unexpectedly".to_owned()))
      },
      move |result| Message::Analysed(generation, result),
    )
  }

  fn view(&self) -> Element<'_, Message> {
    let content: Element<'_, Message> = match (&self.loaded, &self.path) {
      (Some(loaded), Some(path)) => {
        Canvas::new(Plot { app: self, loaded, title: file_name(path) }).width(Fill).height(Fill).into()
      }
      _ => {
        let message = match (&self.error, self.analysing) {
          (Some(e), _) => format!("Cannot analyse this file: {e}"),
          (None, true) => "Analysing…".to_owned(),
          (None, false) => "Drop an audio file here".to_owned(),
        };
        center(text(message).size(16).color(Color::from_rgb8(0xaa, 0xaa, 0xaa))).into()
      }
    };

    let mut layers = stack![content];
    if self.loaded.is_some() && (self.analysing || self.error.is_some()) {
      let status = self.error.as_deref().unwrap_or("Analysing…");
      layers = layers.push(
        container(text(status.to_owned()).size(12).color(Color::from_rgb8(0xaa, 0xaa, 0xaa)))
          .align_right(Fill)
          .align_bottom(Fill)
          .padding(12),
      );
    }
    if self.hovering {
      layers = layers.push(center(text("Drop to analyse").size(20)).style(|_| container::Style {
        background: Some(Color { a: 0.7, ..Color::BLACK }.into()),
        border: Border { color: Color::from_rgb8(0x4c, 0x9a, 0xff), width: 3.0, radius: 0.0.into() },
        text_color: Some(Color::WHITE),
        ..Default::default()
      }));
    }

    container(layers).style(|_| container::Style { background: Some(Color::BLACK.into()), ..Default::default() }).into()
  }
}

/// Draws the spectrogram with Spek's axes, header and colour bar.
struct Plot<'a> {
  app: &'a App,
  loaded: &'a Loaded,
  title: String,
}

impl canvas::Program<Message> for Plot<'_> {
  type State = ();

  fn draw(
    &self,
    _state: &(),
    renderer: &Renderer,
    _theme: &Theme,
    bounds: Rectangle,
    _cursor: mouse::Cursor,
  ) -> Vec<Geometry> {
    let geometry = self.app.cache.draw(renderer, bounds.size(), |frame| {
      let l = Layout::new(bounds.width, bounds.height);
      let info = &self.loaded.analysis.info;
      let white = Color::WHITE;
      let bold = Font { weight: font::Weight::Bold, ..Font::DEFAULT };

      label(frame, &self.title, l.title, layout::TITLE_SIZE, bold, Align::Start, Vertical::Top);
      label(
        frame,
        &self.loaded.analysis.description(),
        l.description,
        layout::BODY_SIZE,
        Font::DEFAULT,
        Align::Start,
        Vertical::Top,
      );
      label(frame, "Specal", l.app_name, layout::TITLE_SIZE, bold, Align::Start, Vertical::Top);

      let plot = to_rect(l.plot);
      let bar = to_rect(l.bar);
      if plot.width <= 0.0 || plot.height <= 0.0 {
        return;
      }
      frame.draw_image(plot, &self.loaded.spectrogram);
      frame.draw_image(bar, &self.app.bar);
      let stroke = Stroke::default().with_color(white).with_width(1.0);
      frame.stroke_rectangle(plot.position(), plot.size(), stroke);

      let label_height = layout::LABEL_SIZE * 1.2;
      let time_label_width = ticks::TIME_SAMPLE_LABEL.len() as f32 * layout::LABEL_SIZE * CHAR_WIDTH;

      for t in ticks::time(info.duration, plot.width, time_label_width) {
        let x = plot.x + t.pos * plot.width;
        let y = plot.y + plot.height;
        line(frame, (x, y), (x, y + layout::TICK), stroke);
        label(frame, &t.label, (x, y + layout::GAP), layout::LABEL_SIZE, Font::DEFAULT, Align::Center, Vertical::Top);
      }
      for t in ticks::frequency(info.sample_rate, plot.height, label_height) {
        let y = plot.y + plot.height - t.pos * plot.height;
        line(frame, (plot.x, y), (plot.x - layout::TICK, y), stroke);
        label(
          frame,
          &t.label,
          (plot.x - layout::GAP, y),
          layout::LABEL_SIZE,
          Font::DEFAULT,
          Align::End,
          Vertical::Center,
        );
      }
      for t in ticks::db(self.app.range, bar.height, label_height) {
        let x = bar.x + bar.width;
        let y = bar.y + bar.height - t.pos * bar.height;
        line(frame, (x, y), (x + layout::TICK, y), stroke);
        label(frame, &t.label, (x + layout::GAP, y), layout::LABEL_SIZE, Font::DEFAULT, Align::Start, Vertical::Center);
      }
    });
    vec![geometry]
  }
}

enum Align {
  Start,
  Center,
  End,
}

fn label(frame: &mut Frame, content: &str, at: (f32, f32), size: f32, font: Font, align_x: Align, align_y: Vertical) {
  frame.fill_text(canvas::Text {
    content: content.to_owned(),
    position: Point::new(at.0, at.1),
    color: Color::WHITE,
    size: Pixels(size),
    font,
    align_x: match align_x {
      Align::Start => text::Alignment::Left,
      Align::Center => text::Alignment::Center,
      Align::End => text::Alignment::Right,
    },
    align_y,
    ..canvas::Text::default()
  });
}

fn line(frame: &mut Frame, from: (f32, f32), to: (f32, f32), stroke: Stroke<'_>) {
  frame.stroke(&canvas::Path::line(Point::new(from.0, from.1), Point::new(to.0, to.1)), stroke);
}

fn to_rect(r: layout::Rect) -> Rectangle {
  Rectangle { x: r.x, y: r.y, width: r.width, height: r.height }
}

fn file_name(path: &Path) -> String {
  path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned()
}
