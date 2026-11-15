// Responsibility: Scroll-demo page assembly, clock drivers, auto-scroll interval, and root shell layout.

use riode::prelude::*;
use riode_shader::{Fill, Shadow, Shape, Stroke};

pub const HOT_TITLE: &str = "Aurora — Test figma gen";
pub const HOT_WIDTH: u32 = 800;
pub const HOT_HEIGHT: u32 = 985;
pub const HOT_BG: u32 = 0xFFFFFF;

#[component]
pub fn Page() -> GroupElement {
    view! {
        <g w=1512 h=993 material=Fill::linear_gradient(123.02, [(0.0, 0xCBABE2), (1.0, 0x9DC6EA)])>
            <WallpaperDark w="fill" h="fill" absolute />
            <MenuBarOverDarkWallpaper label="App Name" w="fill" h=45 absolute />
            <g col gap=20 align_h="center" p=20 w=390 absolute x=561 y=337.5 shape=Shape::rounded(26.0) material=[Shadow::drop((0, 16), 33, "#00000066"), Shadow::drop((0, 0), 1, "#000000CC"), Fill::solid(0x1E1E1E)] overlay=Stroke::new(1.0, "#FFFFFF33") clip>
                <g row gap=8 align="center" pr=20 w="fill">
                    <g w=92 h=69 />
                    <g col gap=6 w="fill">
                        <text s=14 weight=700 color=0xFFFFFF font_family="SF Pro" w="fill">"Do you want to keep this new document “Untitled”?"</text>
                        <text s=11 weight=500 color=0xFFFFFF font_family="SF Pro" lh=14 letter_spacing=-0.15 w="fill">"You can choose to save your changes, or delete this document immediately. You can’t undo this action."</text>
                    </g>
                </g>
                <g col gap=12 align_v="center" pr=20 pb=2 w="fill">
                    <g row gap=8 align_v="center" w="fill">
                        <text s=13 color="#FFFFFF8C" font_family="SF Pro" align="right" w=92>"Save As:"</text>
                        <TextFieldsDark3RgValueTyping label="Untitled" w="fill" />
                    </g>
                    <g row gap=8 align_v="center" w="fill">
                        <text s=13 color="#FFFFFF8C" font_family="SF Pro" align="right" w=92>"Tags:"</text>
                        <TextFieldsDark3RgEmpty w="fill" />
                    </g>
                    <g row gap=8 align_v="center" w="fill">
                        <text s=13 color="#FFFFFF8C" font_family="SF Pro" align="right" w=92>"File Format:"</text>
                        <PopUpButtonsDarkContentArea3Rg1Idle label="Rich Text Document" w="fill" />
                    </g>
                    <g row gap=8 align_v="center" w="fill">
                        <text s=13 color="#FFFFFF8C" font_family="SF Pro" align="right" w=92>"Where:"</text>
                        <g row gap=8 align_v="center" w="fill">
                            <PopUpButtonsDarkContentArea3Rg1Idle label="Desktop" w="fill" />
                            <DisclosureButtonsDarkContentArea3RgDown1Idle />
                        </g>
                    </g>
                </g>
                <g row gap=8 align_v="center" w="fill">
                    <ButtonsDarkContentAreaBorderedDestructive3RgActiveOff1Idle label="Delete" />
                    <g row gap=8 align_h="end" align_v="center" w="fill">
                        <ButtonsDarkContentAreaBordered3RgActiveOff1Idle label="Cancel" />
                        <ButtonsLightContentAreaBorderedDefault3RgActiveOff1Idle label="Save" />
                    </g>
                </g>
            </g>
        </g>
    }
}

// ------------------------------------------------------------------ Dependent Components

#[component]
pub fn ButtonsDarkContentAreaBordered3RgActiveOff1Idle(
    #[prop(default = "Cancel")] label: &'static str,
) -> GroupElement {
    view! {
        <g row align="center" py=4 px=16 w=76 shape=Shape::rounded(6.0) material=Fill::solid("#FFFFFF12") clip>
            <text s=13 weight=500 color=0xFFFFFF font_family="SF Pro" align="center" lh=16 w=44>{label}</text>
        </g>
    }
}

#[component]
pub fn ButtonsDarkContentAreaBorderedDestructive3RgActiveOff1Idle(
    #[prop(default = "Delete")] label: &'static str,
) -> GroupElement {
    view! {
        <g row align="center" py=4 px=16 w=76 shape=Shape::rounded(5.0) clip>
            <text s=13 weight=500 color=0xFF4245 font_family="SF Pro" align="center" lh=16 w=40>{label}</text>
            <g w=66 h=24 absolute material=[Fill::solid(0xFF4245), Fill::solid("#FFFFFF24")] opacity=0.2 />
        </g>
    }
}

#[component]
pub fn ButtonsLightContentAreaBorderedDefault3RgActiveOff1Idle(
    #[prop(default = "Save")] label: &'static str,
) -> GroupElement {
    view! {
        <g row align="center" py=4 px=16 w=76 shape=Shape::rounded(6.0) material=[Fill::solid(0x0088FF), Fill::solid("#00000008")] clip>
            <text s=13 weight=500 color=0xFFFFFF font_family="SF Pro" align="center" lh=16 w=31>{label}</text>
        </g>
    }
}

#[component]
pub fn DisclosureButtonsDarkContentArea3RgDown1Idle() -> GroupElement {
    view! {
        <g w=24 h=24 shape=Shape::rounded(6.0) material=Fill::solid("#FFFFFF12") clip>
            <text s=13 weight=600 color=0xFFFFFF font_family="SF Pro" align="center" lh=16 absolute>"􀆈"</text>
        </g>
    }
}

#[component]
pub fn MenuBarOverDarkWallpaper(#[prop(default = "App Name")] label: &'static str) -> GroupElement {
    view! {
        <g row gap=202 align_h="between" align_v="center" pt=10 pr=20 pb=11 pl=10 w=1512>
            <g row align_v="center">
                <MenuBarItemOnDarkerBackgroundAppleLogo />
                <MenuBarItemOnDarkerBackgroundAppNameIdle label="App Name" />
                <MenuBarItemOnDarkerBackgroundItemIdle label="File" />
                <MenuBarItemOnDarkerBackgroundItemIdle label="Edit" />
                <MenuBarItemOnDarkerBackgroundItemIdle label="View" />
                <MenuBarItemOnDarkerBackgroundItemIdle label="Menu" />
                <MenuBarItemOnDarkerBackgroundItemIdle label="Menu" />
                <MenuBarItemOnDarkerBackgroundItemIdle label="Menu" />
                <MenuBarItemOnDarkerBackgroundItemIdle label="Window" />
                <MenuBarItemOnDarkerBackgroundItemIdle label="Help" />
            </g>
            <g row gap=3 align_h="end" align_v="center">
                <g row align_h="end" align_v="center">
                    <g row gap=6 align_v="center">
                        <g row align="center" pb=0.5 w=30>
                            <svg src={include_svg!("Wifi.svg", 16, 12)} w=16.36 h=12 />
                        </g>
                        <g row align_v="center" pr=10 pb=1 pl=6 w=31>
                            <text s=13 weight=600 color="#FFFFFFD9" font_family="SF Pro">"􀊫"</text>
                        </g>
                        <g row align="center" pr=1.5 w=30 h="fill">
                            <g w=13.39 h=13.42>
                                <svg src={include_svg!("Shape.svg", 13, 6)} w=13.14 h=5.89 absolute y=7.52 />
                                <svg src={include_svg!("Shape.svg", 13, 6)} w=13.14 h=6.48 absolute />
                                <svg src={include_svg!("Path.svg", 5, 3)} w=4.92 h=2.92 absolute x=1.84 y=1.78 />
                            </g>
                        </g>
                    </g>
                    <g row align_h="end" align_v="center" pt=4 pb=5 pl=12>
                        <text s=13.1 weight=600 color=0xFFFFFF font_family="SF Pro" align="right" letter_spacing=-0.18 w=63>"Wed Apr 1"</text>
                    </g>
                    <g row align_h="end" align_v="center" pb=1 pl=8>
                        <text s=13.1 weight=600 color=0xFFFFFF font_family="SF Pro" align="right" letter_spacing=-0.18 w=51>"9:41 AM"</text>
                    </g>
                </g>
            </g>
        </g>
    }
}

#[component]
pub fn MenuBarItemOnDarkerBackgroundAppNameIdle(
    #[prop(default = "App Name")] label: &'static str,
) -> GroupElement {
    view! {
        <g row align_v="center" pt=4 pr=9 pb=5 pl=11>
            <text s=13 weight=900 color=0xFFFFFF font_family="SF Pro" letter_spacing=-0.2>{label}</text>
        </g>
    }
}

#[component]
pub fn MenuBarItemOnDarkerBackgroundAppleLogo() -> GroupElement {
    view! {
        <g w=33 h=24>
            <text s=16.3 color=0xFFFFFF font_family="SF Pro" align="center" lh=20 absolute x=9.5 y=1>"􀣺"</text>
        </g>
    }
}

#[component]
pub fn MenuBarItemOnDarkerBackgroundItemIdle(
    #[prop(default = "File")] label: &'static str,
) -> GroupElement {
    view! {
        <g row align_h="end" align_v="center" pt=4 pr=9 pb=5 pl=10>
            <text s=13 weight=600 color=0xFFFFFF font_family="SF Pro">{label}</text>
        </g>
    }
}

#[component]
pub fn PopUpButtonsDarkContentArea3Rg1Idle(
    #[prop(default = "Rich Text Document")] label: &'static str,
) -> GroupElement {
    view! {
        <g row gap=4 align_h="between" align_v="center" pl=12 w=230 h=24 shape=Shape::rounded(6.0) material=Fill::solid("#FFFFFF12") clip>
            <text s=13 weight=500 color=0xFFFFFF font_family="SF Pro" lh=16 w="fill">{label}</text>
            <text s=13 weight=600 color=0xFFFFFF font_family="SF Pro" align="center" lh=16 w=24>"􀆏"</text>
        </g>
    }
}

#[component]
pub fn TextFieldsDark3RgEmpty() -> GroupElement {
    view! {
        <g row gap=3 align_v="center" pr=4 pl=6 w=230 h=24 shape=Shape::rounded(6.0) material=Fill::solid(0x1E1E1E) overlay=Stroke::new(1.0, "#FFFFFF0A") />
    }
}

#[component]
pub fn TextFieldsDark3RgValueTyping(
    #[prop(default = "Untitled")] label: &'static str,
) -> GroupElement {
    view! {
        <g row gap=1 align_v="center" px=6 w=230 h=24 shape=Shape::rounded(6.0) material=Fill::solid(0x1E1E1E) overlay=Stroke::new(1.0, "#FFFFFF0A") clip>
            <g row align_v="center" w="fill">
                <text s=13 weight=500 color=0xFFFFFF font_family="SF Pro">{label}</text>
                <g w=2 h=18 shape=Shape::rounded(2.5) material=Fill::solid(0x0091FF) />
            </g>
            <g w=120 h=24 absolute shape=Shape::rounded(6.0) overlay=Stroke::new(3.5, 0x0091FF) opacity=0.5 />
        </g>
    }
}

#[component]
pub fn WallpaperDark() -> impl IntoElement {
    view! {
        <img src={include_image!("Wallpaper.png", 1512, 993)} w=1512 h=993 />
    }
}
