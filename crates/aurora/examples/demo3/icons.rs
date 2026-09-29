// Single responsibility: Vector icon constants and SVG document loaders for the Optype showcase.

use aurora::prelude::VectorGraphic;
use std::sync::Arc;

pub struct AppIcons {
    pub logo: Arc<VectorGraphic>,
    pub search: Arc<VectorGraphic>,
    pub filter: Arc<VectorGraphic>,
    pub folder: Arc<VectorGraphic>,
    pub storage: Arc<VectorGraphic>,
    pub link: Arc<VectorGraphic>,
    pub star: Arc<VectorGraphic>,
}

impl AppIcons {
    pub fn load() -> Self {
        Self {
            logo: Arc::new(VectorGraphic::from_str(LOGO_SVG).unwrap()),
            search: Arc::new(VectorGraphic::from_str(SEARCH_SVG).unwrap()),
            filter: Arc::new(VectorGraphic::from_str(FILTER_SVG).unwrap()),
            folder: Arc::new(VectorGraphic::from_str(FOLDER_SVG).unwrap()),
            storage: Arc::new(VectorGraphic::from_str(STORAGE_SVG).unwrap()),
            link: Arc::new(VectorGraphic::from_str(LINK_SVG).unwrap()),
            star: Arc::new(VectorGraphic::from_str(STAR_SVG).unwrap()),
        }
    }
}

const LOGO_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2.472 0.263L21.347 0.263A2.358 2.358 0 0 1 23.706 2.622L23.706 21.498A2.358 2.358 0 0 1 21.347 23.857L2.472 23.857A2.358 2.358 0 0 1 0.113 21.498L0.113 2.622A2.358 2.358 0 0 1 2.472 0.263ZM7.192 4.981A2.358 2.358 0 0 0 4.83 7.34A2.358 2.358 0 0 0 7.192 9.701L7.192 9.701A2.358 2.358 0 0 1 9.55 12.06A2.358 2.358 0 0 1 7.192 14.418L7.192 14.418A2.358 2.358 0 0 0 4.83 16.78A2.358 2.358 0 0 0 7.192 19.138L7.192 19.138A2.358 2.358 0 0 0 9.55 16.78L9.55 16.78A2.358 2.358 0 0 1 11.91 14.418L11.91 14.418A2.358 2.358 0 0 0 14.269 12.06L14.269 12.06A2.358 2.358 0 0 0 11.91 9.701L11.91 9.701A2.358 2.358 0 0 1 9.55 7.34L9.55 7.34A2.358 2.358 0 0 0 7.192 4.981ZM16.628 4.981A2.358 2.358 0 0 0 14.269 7.34L14.269 7.34A2.358 2.358 0 0 0 16.628 9.701L16.628 9.701A2.358 2.358 0 0 0 18.989 7.34L18.989 7.34A2.358 2.358 0 0 0 16.628 4.981Z" fill="#000000"/></svg>"##;
const SEARCH_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="M8.501 8.5l2 2m-1-5a4 4 0 1 0-8 0 4 4 0 0 0 8 0" fill="none" stroke="#000000" stroke-width="0.9" stroke-linecap="round"/></svg>"##;
const FILTER_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="M4.429 6.254C3.185 5.324 2.298 4.3 1.814 3.726c-0.15-0.178-0.198-0.309-0.229-0.538-0.102-0.786-0.151-1.178 0.079-1.434S2.302 1.5 3.117 1.5h5.765c0.814 0 1.222 0 1.454 0.254 0.23 0.254 0.179 0.647 0.078 1.432-0.03 0.23-0.078 0.36-0.228 0.539-0.486 0.576-1.374 1.601-2.62 2.533a0.526 0.526 0 0 0-0.2 0.372c-0.123 1.366-0.238 2.114-0.309 2.492-0.114 0.61-0.979 0.978-1.444 1.306-0.275 0.194-0.61-0.037-0.647-0.338a98 98 90 0 1-0.336-3.46 0.526 0.526 0 0 0-0.201-0.376" fill="none" stroke="#000000" stroke-width="0.9" stroke-linecap="round"/></svg>"##;
const FOLDER_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 13.5 13.5"><path d="M4.5 3.937L9.422 3.937C10.607 3.937 11.199 3.937 11.625 4.222C11.809 4.345 11.968 4.503 12.091 4.687C12.375 5.113 12.375 5.705 12.375 6.89C12.375 8.865 12.375 9.853 11.901 10.562C11.696 10.869 11.432 11.134 11.125 11.339C10.417 11.812 9.428 11.812 7.453 11.812L6.75 11.812C4.099 11.812 2.773 11.812 1.949 10.988C1.125 10.165 1.125 8.84 1.125 6.187L1.125 4.468C1.125 3.447 1.125 2.935 1.339 2.552C1.491 2.279 1.717 2.053 1.99 1.901C2.374 1.687 2.885 1.687 3.906 1.687C4.561 1.687 4.888 1.687 5.175 1.795C5.829 2.041 6.099 2.635 6.395 3.225L6.75 3.937" fill="none" stroke="#000000" stroke-width="0.9" stroke-linecap="round"/></svg>"##;
const STORAGE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 11.25 11.25"><path d="M1.875 8.439V2.814m7.5 0v5.625M5.625 4.689c2.071 0 3.75-0.839 3.75-1.875s-1.679-1.875-3.75-1.875-3.75 0.839-3.75 1.875 1.679 1.875 3.75 1.875ZM9.375 8.439c0 1.036-1.679 1.875-3.75 1.875s-3.75-0.839-3.75-1.875" fill="none" stroke="#000000" stroke-width="0.8"/></svg>"##;
const LINK_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 7.5 7.5"><path d="M3.468 0.938c-1.141 0.003-1.738 0.03-2.119 0.411C0.938 1.761 0.938 2.424 0.938 3.75c0 1.326 0 1.989 0.411 2.401S2.424 6.563 3.75 6.563s1.988 0 2.4-0.412c0.382-0.382 0.41-0.979 0.412-2.119m-0.138-2.939l-2.971 2.988m2.97-2.988c-0.154-0.154-1.194-0.14-1.413-0.137m1.413 0.137c0.154 0.155 0.14 1.196 0.138 1.415" fill="none" stroke="#595959" stroke-width="0.8"/></svg>"##;
const STAR_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" fill="#E5A000"/></svg>"##;
