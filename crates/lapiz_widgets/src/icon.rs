use iced_core::{Element, Length, Theme};
use iced_wgpu::Renderer;
use iced_widget::{Svg, svg};

pub use iced_widget::svg::{Catalog, Status, Style, StyleFn};

pub struct Icon<'a> {
    inner: Svg<'a, Theme>,
}

impl<'a> Icon<'a> {
    pub fn new(handle: impl Into<svg::Handle>) -> Self {
        Self {
            inner: Svg::new(handle).width(16).height(16).style(default),
        }
    }

    pub fn size(mut self, size: impl Into<Length> + Copy) -> Self {
        self.inner = self.inner.width(size).height(size);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.inner = self.inner.height(height);
        self
    }

    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self {
        self.inner = self.inner.style(style);
        self
    }

    pub fn class(mut self, class: impl Into<<Theme as Catalog>::Class<'a>>) -> Self {
        self.inner = self.inner.class(class);
        self
    }

    pub fn muted(self) -> Self {
        self.style(muted)
    }

    pub fn accent(self) -> Self {
        self.style(accent)
    }

    pub fn danger(self) -> Self {
        self.style(danger)
    }
}

impl<'a, Message: 'a> From<Icon<'a>> for Element<'a, Message, Theme, Renderer> {
    fn from(value: Icon<'a>) -> Self {
        value.inner.into()
    }
}

pub fn default(theme: &Theme, _status: Status) -> Style {
    Style {
        color: Some(theme.extended_palette().background.base.text),
    }
}

pub fn muted(theme: &Theme, _status: Status) -> Style {
    Style {
        color: Some(theme.extended_palette().background.weak.text),
    }
}

pub fn accent(theme: &Theme, _status: Status) -> Style {
    Style {
        color: Some(theme.extended_palette().primary.strong.color),
    }
}

pub fn danger(theme: &Theme, _status: Status) -> Style {
    Style {
        color: Some(theme.extended_palette().danger.base.color),
    }
}

macro_rules! icons {
    ($($name:ident => $path:literal),* $(,)?) => {
        $(
            pub fn $name<'a>() -> Icon<'a> {
                Icon::new(svg::Handle::from_memory(include_bytes!($path).as_slice()))
            }
        )*

        pub const ALL: &[(&str, &[u8])] = &[
            $((stringify!($name), include_bytes!($path).as_slice()),)*
        ];
    };
}

icons! {
    airbrush => "../assets/icons/airbrush.svg",
    alpha_lock => "../assets/icons/alpha_lock.svg",
    arrow_right => "../assets/icons/arrow_right.svg",
    blend => "../assets/icons/blend.svg",
    blender => "../assets/icons/blender.svg",
    blur => "../assets/icons/blur.svg",
    brush => "../assets/icons/brush.svg",
    burn => "../assets/icons/burn.svg",
    canvas_size => "../assets/icons/canvas_size.svg",
    caret_down => "../assets/icons/caret_down.svg",
    caret_right => "../assets/icons/caret_right.svg",
    check => "../assets/icons/check.svg",
    chevron_down => "../assets/icons/chevron_down.svg",
    chevron_left => "../assets/icons/chevron_left.svg",
    chevron_right => "../assets/icons/chevron_right.svg",
    chevron_up => "../assets/icons/chevron_up.svg",
    clock => "../assets/icons/clock.svg",
    close => "../assets/icons/close.svg",
    cloud => "../assets/icons/cloud.svg",
    copy => "../assets/icons/copy.svg",
    cpu => "../assets/icons/cpu.svg",
    crop => "../assets/icons/crop.svg",
    curve => "../assets/icons/curve.svg",
    dock_bottom => "../assets/icons/dock_bottom.svg",
    dock_left => "../assets/icons/dock_left.svg",
    dock_right => "../assets/icons/dock_right.svg",
    dodge => "../assets/icons/dodge.svg",
    ellipse => "../assets/icons/ellipse.svg",
    ellipse_select => "../assets/icons/ellipse_select.svg",
    eraser => "../assets/icons/eraser.svg",
    export => "../assets/icons/export.svg",
    eye => "../assets/icons/eye.svg",
    eye_off => "../assets/icons/eye_off.svg",
    eyedropper => "../assets/icons/eyedropper.svg",
    file => "../assets/icons/file.svg",
    file_new => "../assets/icons/file_new.svg",
    fill => "../assets/icons/fill.svg",
    filter => "../assets/icons/filter.svg",
    folder => "../assets/icons/folder.svg",
    folder_open => "../assets/icons/folder_open.svg",
    gradient => "../assets/icons/gradient.svg",
    grid => "../assets/icons/grid.svg",
    grip => "../assets/icons/grip.svg",
    group => "../assets/icons/group.svg",
    hand => "../assets/icons/hand.svg",
    history => "../assets/icons/history.svg",
    import => "../assets/icons/import.svg",
    info => "../assets/icons/info.svg",
    keyboard => "../assets/icons/keyboard.svg",
    lasso => "../assets/icons/lasso.svg",
    layers => "../assets/icons/layers.svg",
    line => "../assets/icons/line.svg",
    link => "../assets/icons/link.svg",
    lock => "../assets/icons/lock.svg",
    magic_wand => "../assets/icons/magic_wand.svg",
    magnet => "../assets/icons/magnet.svg",
    mask => "../assets/icons/mask.svg",
    merge => "../assets/icons/merge.svg",
    minus => "../assets/icons/minus.svg",
    monitor => "../assets/icons/monitor.svg",
    moon => "../assets/icons/moon.svg",
    more => "../assets/icons/more.svg",
    move_tool => "../assets/icons/move.svg",
    nodes => "../assets/icons/nodes.svg",
    opacity => "../assets/icons/opacity.svg",
    palette => "../assets/icons/palette.svg",
    pencil => "../assets/icons/pencil.svg",
    perspective => "../assets/icons/perspective.svg",
    pin => "../assets/icons/pin.svg",
    play => "../assets/icons/play.svg",
    plugin => "../assets/icons/plugin.svg",
    plus => "../assets/icons/plus.svg",
    poly_lasso => "../assets/icons/poly_lasso.svg",
    polygon => "../assets/icons/polygon.svg",
    rect => "../assets/icons/rect.svg",
    rect_select => "../assets/icons/rect_select.svg",
    redo => "../assets/icons/redo.svg",
    reference => "../assets/icons/reference.svg",
    refresh => "../assets/icons/refresh.svg",
    ruler => "../assets/icons/ruler.svg",
    save => "../assets/icons/save.svg",
    search => "../assets/icons/search.svg",
    settings => "../assets/icons/settings.svg",
    sharpen => "../assets/icons/sharpen.svg",
    sliders => "../assets/icons/sliders.svg",
    smudge => "../assets/icons/smudge.svg",
    stamp => "../assets/icons/stamp.svg",
    star => "../assets/icons/star.svg",
    sun => "../assets/icons/sun.svg",
    swatches => "../assets/icons/swatches.svg",
    symmetry => "../assets/icons/symmetry.svg",
    target => "../assets/icons/target.svg",
    text => "../assets/icons/text.svg",
    transform => "../assets/icons/transform.svg",
    trash => "../assets/icons/trash.svg",
    undo => "../assets/icons/undo.svg",
    unlock => "../assets/icons/unlock.svg",
    user => "../assets/icons/user.svg",
    warning => "../assets/icons/warning.svg",
    win_close => "../assets/icons/win_close.svg",
    win_maximize => "../assets/icons/win_maximize.svg",
    win_minimize => "../assets/icons/win_minimize.svg",
    win_restore => "../assets/icons/win_restore.svg",
    zoom => "../assets/icons/zoom.svg",
}

pub struct IconGroup {
    pub name: &'static str,
    pub icons: &'static [(&'static str, &'static [u8])],
}

const GROUP_PAINTING_TOOLS: &[(&str, &[u8])] = &[
    (
        "pencil",
        include_bytes!("../assets/icons/pencil.svg").as_slice(),
    ),
    (
        "brush",
        include_bytes!("../assets/icons/brush.svg").as_slice(),
    ),
    (
        "airbrush",
        include_bytes!("../assets/icons/airbrush.svg").as_slice(),
    ),
    (
        "eraser",
        include_bytes!("../assets/icons/eraser.svg").as_slice(),
    ),
    (
        "blender",
        include_bytes!("../assets/icons/blender.svg").as_slice(),
    ),
    (
        "smudge",
        include_bytes!("../assets/icons/smudge.svg").as_slice(),
    ),
    (
        "fill",
        include_bytes!("../assets/icons/fill.svg").as_slice(),
    ),
    (
        "gradient",
        include_bytes!("../assets/icons/gradient.svg").as_slice(),
    ),
    (
        "stamp",
        include_bytes!("../assets/icons/stamp.svg").as_slice(),
    ),
    (
        "dodge",
        include_bytes!("../assets/icons/dodge.svg").as_slice(),
    ),
    (
        "burn",
        include_bytes!("../assets/icons/burn.svg").as_slice(),
    ),
    (
        "blur",
        include_bytes!("../assets/icons/blur.svg").as_slice(),
    ),
    (
        "sharpen",
        include_bytes!("../assets/icons/sharpen.svg").as_slice(),
    ),
];

const GROUP_SELECTION_SHAPE: &[(&str, &[u8])] = &[
    (
        "rect-select",
        include_bytes!("../assets/icons/rect_select.svg").as_slice(),
    ),
    (
        "ellipse-select",
        include_bytes!("../assets/icons/ellipse_select.svg").as_slice(),
    ),
    (
        "lasso",
        include_bytes!("../assets/icons/lasso.svg").as_slice(),
    ),
    (
        "poly-lasso",
        include_bytes!("../assets/icons/poly_lasso.svg").as_slice(),
    ),
    (
        "magic-wand",
        include_bytes!("../assets/icons/magic_wand.svg").as_slice(),
    ),
    (
        "line",
        include_bytes!("../assets/icons/line.svg").as_slice(),
    ),
    (
        "curve",
        include_bytes!("../assets/icons/curve.svg").as_slice(),
    ),
    (
        "rect",
        include_bytes!("../assets/icons/rect.svg").as_slice(),
    ),
    (
        "ellipse",
        include_bytes!("../assets/icons/ellipse.svg").as_slice(),
    ),
    (
        "polygon",
        include_bytes!("../assets/icons/polygon.svg").as_slice(),
    ),
    (
        "text",
        include_bytes!("../assets/icons/text.svg").as_slice(),
    ),
    (
        "transform",
        include_bytes!("../assets/icons/transform.svg").as_slice(),
    ),
    (
        "crop",
        include_bytes!("../assets/icons/crop.svg").as_slice(),
    ),
    (
        "move",
        include_bytes!("../assets/icons/move.svg").as_slice(),
    ),
];

const GROUP_VIEW_ASSIST: &[(&str, &[u8])] = &[
    (
        "eyedropper",
        include_bytes!("../assets/icons/eyedropper.svg").as_slice(),
    ),
    (
        "zoom",
        include_bytes!("../assets/icons/zoom.svg").as_slice(),
    ),
    (
        "hand",
        include_bytes!("../assets/icons/hand.svg").as_slice(),
    ),
    (
        "symmetry",
        include_bytes!("../assets/icons/symmetry.svg").as_slice(),
    ),
    (
        "perspective",
        include_bytes!("../assets/icons/perspective.svg").as_slice(),
    ),
    (
        "mask",
        include_bytes!("../assets/icons/mask.svg").as_slice(),
    ),
    (
        "reference",
        include_bytes!("../assets/icons/reference.svg").as_slice(),
    ),
    (
        "ruler",
        include_bytes!("../assets/icons/ruler.svg").as_slice(),
    ),
    (
        "magnet",
        include_bytes!("../assets/icons/magnet.svg").as_slice(),
    ),
    (
        "target",
        include_bytes!("../assets/icons/target.svg").as_slice(),
    ),
    (
        "grid",
        include_bytes!("../assets/icons/grid.svg").as_slice(),
    ),
    (
        "canvas-size",
        include_bytes!("../assets/icons/canvas_size.svg").as_slice(),
    ),
];

const GROUP_DOCUMENT: &[(&str, &[u8])] = &[
    (
        "file",
        include_bytes!("../assets/icons/file.svg").as_slice(),
    ),
    (
        "file-new",
        include_bytes!("../assets/icons/file_new.svg").as_slice(),
    ),
    (
        "folder",
        include_bytes!("../assets/icons/folder.svg").as_slice(),
    ),
    (
        "folder-open",
        include_bytes!("../assets/icons/folder_open.svg").as_slice(),
    ),
    (
        "save",
        include_bytes!("../assets/icons/save.svg").as_slice(),
    ),
    (
        "export",
        include_bytes!("../assets/icons/export.svg").as_slice(),
    ),
    (
        "import",
        include_bytes!("../assets/icons/import.svg").as_slice(),
    ),
    (
        "undo",
        include_bytes!("../assets/icons/undo.svg").as_slice(),
    ),
    (
        "redo",
        include_bytes!("../assets/icons/redo.svg").as_slice(),
    ),
    (
        "history",
        include_bytes!("../assets/icons/history.svg").as_slice(),
    ),
    (
        "copy",
        include_bytes!("../assets/icons/copy.svg").as_slice(),
    ),
    (
        "trash",
        include_bytes!("../assets/icons/trash.svg").as_slice(),
    ),
    (
        "merge",
        include_bytes!("../assets/icons/merge.svg").as_slice(),
    ),
    (
        "group",
        include_bytes!("../assets/icons/group.svg").as_slice(),
    ),
];

const GROUP_INTERFACE: &[(&str, &[u8])] = &[
    (
        "layers",
        include_bytes!("../assets/icons/layers.svg").as_slice(),
    ),
    (
        "nodes",
        include_bytes!("../assets/icons/nodes.svg").as_slice(),
    ),
    (
        "palette",
        include_bytes!("../assets/icons/palette.svg").as_slice(),
    ),
    (
        "swatches",
        include_bytes!("../assets/icons/swatches.svg").as_slice(),
    ),
    (
        "sliders",
        include_bytes!("../assets/icons/sliders.svg").as_slice(),
    ),
    (
        "settings",
        include_bytes!("../assets/icons/settings.svg").as_slice(),
    ),
    (
        "search",
        include_bytes!("../assets/icons/search.svg").as_slice(),
    ),
    (
        "filter",
        include_bytes!("../assets/icons/filter.svg").as_slice(),
    ),
    (
        "plus",
        include_bytes!("../assets/icons/plus.svg").as_slice(),
    ),
    (
        "minus",
        include_bytes!("../assets/icons/minus.svg").as_slice(),
    ),
    (
        "close",
        include_bytes!("../assets/icons/close.svg").as_slice(),
    ),
    (
        "check",
        include_bytes!("../assets/icons/check.svg").as_slice(),
    ),
    (
        "chevron-down",
        include_bytes!("../assets/icons/chevron_down.svg").as_slice(),
    ),
    (
        "chevron-up",
        include_bytes!("../assets/icons/chevron_up.svg").as_slice(),
    ),
    (
        "chevron-left",
        include_bytes!("../assets/icons/chevron_left.svg").as_slice(),
    ),
    (
        "chevron-right",
        include_bytes!("../assets/icons/chevron_right.svg").as_slice(),
    ),
    (
        "caret-down",
        include_bytes!("../assets/icons/caret_down.svg").as_slice(),
    ),
    (
        "caret-right",
        include_bytes!("../assets/icons/caret_right.svg").as_slice(),
    ),
    (
        "arrow-right",
        include_bytes!("../assets/icons/arrow_right.svg").as_slice(),
    ),
    (
        "more",
        include_bytes!("../assets/icons/more.svg").as_slice(),
    ),
    (
        "grip",
        include_bytes!("../assets/icons/grip.svg").as_slice(),
    ),
    ("pin", include_bytes!("../assets/icons/pin.svg").as_slice()),
    (
        "refresh",
        include_bytes!("../assets/icons/refresh.svg").as_slice(),
    ),
    (
        "play",
        include_bytes!("../assets/icons/play.svg").as_slice(),
    ),
];

const GROUP_STATUS_SYSTEM: &[(&str, &[u8])] = &[
    ("eye", include_bytes!("../assets/icons/eye.svg").as_slice()),
    (
        "eye-off",
        include_bytes!("../assets/icons/eye_off.svg").as_slice(),
    ),
    (
        "lock",
        include_bytes!("../assets/icons/lock.svg").as_slice(),
    ),
    (
        "unlock",
        include_bytes!("../assets/icons/unlock.svg").as_slice(),
    ),
    (
        "alpha-lock",
        include_bytes!("../assets/icons/alpha_lock.svg").as_slice(),
    ),
    (
        "opacity",
        include_bytes!("../assets/icons/opacity.svg").as_slice(),
    ),
    (
        "blend",
        include_bytes!("../assets/icons/blend.svg").as_slice(),
    ),
    ("sun", include_bytes!("../assets/icons/sun.svg").as_slice()),
    (
        "moon",
        include_bytes!("../assets/icons/moon.svg").as_slice(),
    ),
    (
        "warning",
        include_bytes!("../assets/icons/warning.svg").as_slice(),
    ),
    (
        "info",
        include_bytes!("../assets/icons/info.svg").as_slice(),
    ),
    (
        "link",
        include_bytes!("../assets/icons/link.svg").as_slice(),
    ),
    (
        "monitor",
        include_bytes!("../assets/icons/monitor.svg").as_slice(),
    ),
    (
        "keyboard",
        include_bytes!("../assets/icons/keyboard.svg").as_slice(),
    ),
    ("cpu", include_bytes!("../assets/icons/cpu.svg").as_slice()),
    (
        "clock",
        include_bytes!("../assets/icons/clock.svg").as_slice(),
    ),
    (
        "star",
        include_bytes!("../assets/icons/star.svg").as_slice(),
    ),
    (
        "plugin",
        include_bytes!("../assets/icons/plugin.svg").as_slice(),
    ),
    (
        "user",
        include_bytes!("../assets/icons/user.svg").as_slice(),
    ),
    (
        "cloud",
        include_bytes!("../assets/icons/cloud.svg").as_slice(),
    ),
    (
        "dock-left",
        include_bytes!("../assets/icons/dock_left.svg").as_slice(),
    ),
    (
        "dock-right",
        include_bytes!("../assets/icons/dock_right.svg").as_slice(),
    ),
    (
        "dock-bottom",
        include_bytes!("../assets/icons/dock_bottom.svg").as_slice(),
    ),
    (
        "win-minimize",
        include_bytes!("../assets/icons/win_minimize.svg").as_slice(),
    ),
    (
        "win-maximize",
        include_bytes!("../assets/icons/win_maximize.svg").as_slice(),
    ),
    (
        "win-restore",
        include_bytes!("../assets/icons/win_restore.svg").as_slice(),
    ),
    (
        "win-close",
        include_bytes!("../assets/icons/win_close.svg").as_slice(),
    ),
];

pub const ICON_GROUPS: &[IconGroup] = &[
    IconGroup {
        name: "Painting tools",
        icons: GROUP_PAINTING_TOOLS,
    },
    IconGroup {
        name: "Selection & shape",
        icons: GROUP_SELECTION_SHAPE,
    },
    IconGroup {
        name: "View & assist",
        icons: GROUP_VIEW_ASSIST,
    },
    IconGroup {
        name: "Document",
        icons: GROUP_DOCUMENT,
    },
    IconGroup {
        name: "Interface",
        icons: GROUP_INTERFACE,
    },
    IconGroup {
        name: "Status & system",
        icons: GROUP_STATUS_SYSTEM,
    },
];

pub const ALL_ICONS: &[(&str, &[u8])] = ALL;
