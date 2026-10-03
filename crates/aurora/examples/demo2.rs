// Single responsibility: Self-contained interactive runner for the Paper HUD Dock design.

use aurora::prelude::*;
use std::sync::Arc;

const WIDTH: u32 = 480;
const HEIGHT: u32 = 130;

struct Icons {
    handle: Arc<VectorGraphic>,
    pen: Arc<VectorGraphic>,
    pencil: Arc<VectorGraphic>,
    brush: Arc<VectorGraphic>,
    shape: Arc<VectorGraphic>,
    text: Arc<VectorGraphic>,
    lasso: Arc<VectorGraphic>,
    trash: Arc<VectorGraphic>,
    more: Arc<VectorGraphic>,
    arrow: Arc<VectorGraphic>,
    sub_line: Arc<VectorGraphic>,
    sub_diag: Arc<VectorGraphic>,
    sub_rect: Arc<VectorGraphic>,
    sub_circ: Arc<VectorGraphic>,
}

impl Icons {
    fn load() -> Self {
        Self {
            handle: Arc::new(VectorGraphic::from_str(HANDLE_SVG).unwrap()),
            pen: Arc::new(VectorGraphic::from_str(PEN_SVG).unwrap()),
            pencil: Arc::new(VectorGraphic::from_str(PENCIL_SVG).unwrap()),
            brush: Arc::new(VectorGraphic::from_str(BRUSH_SVG).unwrap()),
            shape: Arc::new(VectorGraphic::from_str(SHAPE_SVG).unwrap()),
            text: Arc::new(VectorGraphic::from_str(TEXT_SVG).unwrap()),
            lasso: Arc::new(VectorGraphic::from_str(LASSO_SVG).unwrap()),
            trash: Arc::new(VectorGraphic::from_str(TRASH_SVG).unwrap()),
            more: Arc::new(VectorGraphic::from_str(MORE_SVG).unwrap()),
            arrow: Arc::new(VectorGraphic::from_str(ARROW_SVG).unwrap()),
            sub_line: Arc::new(VectorGraphic::from_str(SUB_LINE).unwrap()),
            sub_diag: Arc::new(VectorGraphic::from_str(SUB_DIAG).unwrap()),
            sub_rect: Arc::new(VectorGraphic::from_str(SUB_RECT).unwrap()),
            sub_circ: Arc::new(VectorGraphic::from_str(SUB_CIRC).unwrap()),
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let icons = Icons::load();

    App::config()
        .title("Aurora — Demo 2 (Paper HUD Dock)")
        .size(WIDTH, HEIGHT)
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(0x141414))
        .run(move || build_ui(&icons));
}

fn build_ui(icons: &Icons) -> impl IntoElement {
    group().direction(Direction::Vertical)
        .width(Size::fill()).height(Size::fill())
        .fill(Color::hex(0x444444))
        .alignment(Alignment::Center).distribution(Distribution::Center)
        .children([
            group().direction(Direction::Vertical)
                .width(461.0).height(108.0)
                .children([
                    group().direction(Direction::Horizontal)
                        .width(Size::fit())
                        .height(48.0)
                        .fill(Color::BLACK)
                        .radius(14.0)
                        .gap(6.0)
                        .margin(Margin::all(3.0))
                        .alignment(Alignment::Center)
                        .children((
                            handle(&icons.handle),
                            group().direction(Direction::Horizontal).width(Size::fit()).height(42.0).gap(10.0).children((
                                btn_arrow(&icons.pen, &icons.arrow, "q"),
                                btn_active(&icons.pencil, "w"),
                                btn(&icons.brush, "e"),
                                btn_arrow(&icons.shape, &icons.arrow, "r"),
                                btn(&icons.text, "t"),
                                btn(&icons.lasso, "y"),
                            )),
                            divider(),
                            group().direction(Direction::Horizontal).width(Size::fit()).height(42.0).gap(4.0).children((
                                btn_action(&icons.trash),
                                btn_action(&icons.more),
                            )),
                        )),
                    group().direction(Direction::Horizontal)
                        .width(Size::fit())
                        .height(47.0)
                        .fill(Color::BLACK)
                        .radius(14.0)
                        .gap(10.0)
                        .margin(Margin::left(117.5))
                        .margin(Margin::top(10.0))
                        .alignment(Alignment::Center)
                        .children((
                            btn(&icons.sub_line, "a"),
                            btn(&icons.sub_diag, "s"),
                            btn(&icons.sub_rect, "d"),
                            btn(&icons.sub_circ, "f"),
                        )),
                ]),
        ])
}

fn btn(icon: &Arc<VectorGraphic>, k: &'static str) -> impl IntoElement {
    group().width(42.0).height(42.0).radius(12.0).alignment(Alignment::Center).distribution(Distribution::Center).children((
        svg(icon).size(28.0, 28.0),
        text(k).size(10.0).color(Color::rgb(0.4, 0.4, 0.4)).anchor(Anchor::BottomRight),
    ))
}

fn btn_arrow(icon: &Arc<VectorGraphic>, arrow: &Arc<VectorGraphic>, k: &'static str) -> impl IntoElement {
    group().width(46.0).height(42.0).radius(12.0).alignment(Alignment::Center).distribution(Distribution::Center).children((
        svg(icon).size(28.0, 28.0),
        svg(arrow).size(6.0, 6.0).anchor(Anchor::BottomRight).margin(Margin::sides(2.0, 2.0, 2.0, 2.0)),
        text(k).size(10.0).color(Color::rgb(0.4, 0.4, 0.4)).anchor(Anchor::BottomRight).margin(Margin::right(8.0)),
    ))
}

fn btn_active(icon: &Arc<VectorGraphic>, k: &'static str) -> impl IntoElement {
    group()
        .width(42.0).height(42.0)
        .radius(8.0)
        .fill(Color::hex_alpha(0x57FA58, 0.13))
        .shadows([Shadow::inset(0.0, 2.0, 3.0, Color::hex_alpha(0x54F554, 0.18))])
        .alignment(Alignment::Center).distribution(Distribution::Center)
        .children((
            svg(icon).size(28.0, 28.0),
            text(k).size(10.0).color(Color::hex(0x226F22)).anchor(Anchor::BottomRight),
        ))
}

fn btn_action(icon: &Arc<VectorGraphic>) -> impl IntoElement {
    group().width(42.0).height(42.0).radius(12.0).alignment(Alignment::Center).distribution(Distribution::Center).children([svg(icon).size(28.0, 28.0)])
}

fn handle(icon: &Arc<VectorGraphic>) -> impl IntoElement {
    group().width(36.0).height(42.0).alignment(Alignment::Center).distribution(Distribution::Center).children([svg(icon).size(28.0, 28.0)])
}

fn divider() -> impl IntoElement {
    group().width(1.0).height(30.0).fill(Color::white_alpha(0.15)).margin(Margin::x(4.0))
}

const HANDLE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><path d="M13.471 16.344C13.471 16.882 13.036 17.318 12.496 17.318C11.959 17.318 11.524 16.882 11.524 16.344C11.524 15.806 11.959 15.37 12.496 15.37C13.036 15.37 13.471 15.806 13.471 16.344M8.601 10.5C8.601 11.038 8.167 11.474 7.626 11.474C7.089 11.474 6.655 11.038 6.655 10.5C6.655 9.962 7.089 9.526 7.626 9.526C8.167 9.526 8.601 9.962 8.601 10.5M13.471 4.656C13.471 5.194 13.036 5.63 12.496 5.63C11.959 5.63 11.524 5.194 11.524 4.656C11.524 4.118 11.959 3.682 12.496 3.682C13.036 3.682 13.471 4.118 13.471 4.656M13.471 10.5C13.471 11.038 13.036 11.474 12.496 11.474C11.959 11.474 11.524 11.038 11.524 10.5C11.524 9.962 11.959 9.526 12.496 9.526C13.036 9.526 13.471 9.962 13.471 10.5M8.601 4.656C8.601 5.194 8.167 5.63 7.626 5.63C7.089 5.63 6.655 5.194 6.655 4.656C6.655 4.118 7.089 3.682 7.626 3.682C8.167 3.682 8.601 4.118 8.601 4.656M8.601 16.344C8.601 16.882 8.167 17.318 7.626 17.318C7.089 17.318 6.655 16.882 6.655 16.344C6.655 15.806 7.089 15.37 7.626 15.37C8.167 15.37 8.601 15.806 8.601 16.344" fill="none" stroke="#626262" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const PEN_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><path d="M8.989 5.434l4.148 1.622c2.392 0.937 3.589 1.405 3.549 2.147-0.041 0.742-1.288 1.084-3.785 1.764-0.743 0.203-1.115 0.305-1.373 0.561s-0.359 0.63-0.561 1.373c-0.68 2.497-1.022 3.745-1.764 3.785s-1.21-1.156-2.147-3.549L5.434 8.989C4.453 6.484 3.963 5.233 4.598 4.598c0.635-0.635 1.888-0.145 4.391 0.836Z" fill="none" stroke="#FFA000" stroke-width="0.9" stroke-linejoin="round"/></svg>"##;
const PENCIL_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><path d="M12.907 5.353c0.504-0.504 0.756-0.756 1.036-0.878 0.404-0.174 0.86-0.174 1.263 0 0.28 0.12 0.532 0.373 1.036 0.878s0.756 0.757 0.878 1.036c0.174 0.404 0.174 0.86 0 1.263-0.12 0.28-0.373 0.532-0.878 1.036l-4.316 4.318c-1.064 1.064-1.595 1.595-2.261 1.91s-1.414 0.389-2.912 0.537L6.076 15.519l0.067-0.677c0.148-1.497 0.222-2.246 0.536-2.912s0.847-1.198 1.91-2.261z" fill="none" stroke="#57FA58" stroke-width="0.9" stroke-linejoin="round"/></svg>"##;
const BRUSH_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><path d="M7.075 12.83l1.473 1.473m-1.473-1.473L4.165 15.82h2.786l1.597-1.517m-1.473-1.473c-0.271-0.271-0.266-0.711-0.031-1.013 0.552-0.706 0.753-1.345 0.811-1.821 0.065-0.524 0.191-1.09 0.565-1.463l0.621-0.619m-0.493 6.389c0.271 0.271 0.711 0.266 1.013 0.031 0.706-0.552 1.345-0.753 1.821-0.811 0.524-0.065 1.09-0.191 1.463-0.565l0.62-0.62m0 0l-4.423-4.423m4.423 4.423a0.697 0.697 0 0 0 0.983 0l2.949-2.95M9.041 7.914a0.697 0.697 0 0 1 0-0.984L11.99 3.981" fill="none" stroke="#FAFF20" stroke-width="0.9" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const SHAPE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><path d="M4.38 10.5c0-2.884 0-4.327 0.896-5.224S7.615 4.38 10.5 4.38c2.884 0 4.327 0 5.224 0.896S16.62 7.615 16.62 10.5c0 2.884 0 4.327-0.896 5.224S13.385 16.62 10.5 16.62c-2.884 0-4.327 0-5.224-0.896S4.38 13.385 4.38 10.5Z" fill="none" stroke="#00AEFF" stroke-width="0.9"/></svg>"##;
const TEXT_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><path d="M15.145 5.142L15.145 15.859M11.214 14.072L9.836 10.857M16.93 16.93C16.335 16.934 15.5 16.573 15.145 15.859M4.071 14.072L5.448 10.857M15.145 15.859C14.786 16.573 13.953 16.93 13.358 16.93M5.448 10.857L6.944 7.366C7.081 7.05 7.319 6.928 7.643 6.928C7.967 6.928 8.206 7.05 8.341 7.366L9.836 10.857M16.216 10.5L14.072 10.5M5.448 10.857L9.836 10.857M13.358 4.071C13.955 4.066 14.788 4.428 15.145 5.142M15.145 5.142C15.502 4.426 16.336 4.071 16.93 4.071" fill="none" stroke="#00DEB3" stroke-width="0.9" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const LASSO_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><path d="M8.293 7.645l-2.631 2.691c-0.791 0.809-1.187 1.213-1.24 1.706q-0.018 0.158 0 0.314c0.054 0.494 0.449 0.897 1.24 1.707l0.1 0.103c0.422 0.432 0.634 0.648 0.886 0.792q0.222 0.127 0.466 0.198c0.28 0.079 0.579 0.079 1.179 0.08 0.599 0 0.901 0 1.18-0.08q0.245-0.07 0.465-0.197c0.252-0.145 0.464-0.362 0.886-0.793l1.924-1.967M8.293 7.645l2.424-2.473C11.654 4.214 12.124 3.736 12.707 3.736s1.052 0.48 1.99 1.437l0.502 0.515C16.126 6.633 16.588 7.106 16.588 7.692s-0.463 1.059-1.389 2.005l-2.451 2.502M8.293 7.645l4.455 4.554M9.147 17.264h7.441" fill="none" stroke="#FF64D4" stroke-width="0.9" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const TRASH_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><path d="M15.492 6.173l-0.413 6.673c-0.105 1.705-0.158 2.557-0.585 3.171a2.662 2.662 90 0 1-0.798 0.75c-0.637 0.389-1.491 0.389-3.199 0.39-1.71 0-2.566 0-3.205-0.39a2.662 2.662 90 0 1-0.799-0.752c-0.428-0.614-0.48-1.468-0.581-3.176L5.508 6.173M4.509 6.173h11.982m-3.291 0l-0.454-0.937c-0.302-0.623-0.453-0.933-0.714-1.128a1.331 1.331 90 0 0-0.183-0.114C11.561 3.843 11.215 3.843 10.523 3.843c-0.711 0-1.064 0-1.358 0.156a1.331 1.331 90 0 0-0.184 0.121c-0.263 0.202-0.41 0.525-0.705 1.169L7.873 6.173" fill="none" stroke="#FF5847" stroke-width="0.9" stroke-linecap="round"/></svg>"##;
const MORE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><path d="M10.498 10.938V10.5m0-4.812V5.25m0 10.938V15.75m0.875-4.812a0.875 0.875 0 1 0-1.75 0 0.875 0.875 0 0 0 1.75 0m0-5.25a0.875 0.875 0 1 0-1.75 0 0.875 0.875 0 0 0 1.75 0m0 10.5a0.875 0.875 0 1 0-1.75 0 0.875 0.875 0 0 0 1.75 0" fill="none" stroke="#626262" stroke-width="0.9" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const ARROW_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4.5 4.5"><path d="M0.344 1.966l1.5 1.5c0.226 0.226 0.59 0.226 0.816 0l1.499-1.5c0.364-0.364 0.104-0.99-0.411-0.99H0.749c-0.515 0-0.77 0.625-0.405 0.99" fill="#FFFFFF"/></svg>"##;
const SUB_LINE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><path d="M11.998 9.161L5.25 15.75M13.797 5.075L12.685 5.175C10.882 5.34 9.981 5.421 9.769 6.006C9.557 6.593 10.197 7.232 11.476 8.512L12.488 9.524C13.768 10.803 14.407 11.443 14.993 11.231C15.579 11.018 15.66 10.118 15.825 8.315L15.925 7.203C16.024 6.118 16.074 5.575 15.749 5.25C15.425 4.926 14.882 4.975 13.797 5.075" fill="none" stroke="#FFFFFF" stroke-width="0.9" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const SUB_DIAG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><path d="M4.396 15.926L16.604 5.074" fill="none" stroke="#FFFFFF" stroke-width="0.9" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const SUB_RECT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><path d="M4.38 10.5c0-2.884 0-4.327 0.896-5.224S7.615 4.38 10.5 4.38c2.884 0 4.327 0 5.224 0.896S16.62 7.615 16.62 10.5c0 2.884 0 4.327-0.896 5.224S13.385 16.62 10.5 16.62c-2.884 0-4.327 0-5.224-0.896S4.38 13.385 4.38 10.5Z" fill="none" stroke="#FFFFFF" stroke-width="0.9"/></svg>"##;
const SUB_CIRC: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><circle cx="10.5" cy="10.5" r="5.625" fill="none" stroke="#FFFFFF" stroke-width="0.9" stroke-linejoin="round"/></svg>"##;
