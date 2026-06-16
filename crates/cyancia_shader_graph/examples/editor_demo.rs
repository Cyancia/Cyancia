use std::collections::HashMap;

use cyancia_shader_graph::{
    editor::GraphEditor,
    graph::{Graph, GraphData, GraphResources},
    wgsl_std::{
        builtin_nodes, builtin_types,
        nodes::{GraphInputNode, GraphOutputNode},
    },
};
use gpui::{
    Action, App, AppContext, Context, Entity, IntoElement, Menu, MenuItem, ParentElement, Render,
    SharedString, Styled, Window, WindowOptions, div,
};
use gpui_component::{
    ActiveTheme, GlobalState, Root, Theme, ThemeRegistry, TitleBar, menu::AppMenuBar,
};
use gpui_platform::application;

#[derive(Default, Clone)]
struct DemoData {}

impl GraphData for DemoData {}

struct DemoEditor {
    menu_bar: Entity<AppMenuBar>,
    editor: Entity<GraphEditor<DemoData>>,
}

impl DemoEditor {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut nodes = builtin_nodes();
        nodes.register::<GraphInputNode>();
        nodes.register::<GraphOutputNode>();

        let graph = cx.new(|_| Graph::new(GraphResources::default(), builtin_types().into()));
        Self {
            menu_bar: menu_bar_init(cx),
            editor: cx.new(|cx| GraphEditor::new(graph.clone(), nodes.into(), cx)),
        }
    }
}

impl Render for DemoEditor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .h_full()
            .child(TitleBar::new().child(self.menu_bar.clone()))
            .child(self.editor.clone())
    }
}

fn update_app_menu(app_menu_bar: Entity<AppMenuBar>, cx: &mut App) {
    cx.set_menus(build_menus(cx));
    let menus = build_menus(cx)
        .into_iter()
        .map(|menu| menu.owned())
        .collect();
    GlobalState::global_mut(cx).set_app_menus(menus);

    app_menu_bar.update(cx, |menu_bar, cx| {
        menu_bar.reload(cx);
    });
}

fn build_menus(cx: &App) -> Vec<Menu> {
    vec![Menu {
        name: "Themes".into(),
        items: {
            let themes = cx.global::<ThemeRegistry>();
            let current_name = cx.theme().theme_name();

            themes
                .sorted_themes()
                .iter()
                .map(|theme| {
                    MenuItem::action(theme.name.clone(), SwitchThemeAction(theme.name.clone()))
                        .checked(&theme.name == current_name)
                })
                .collect()
        },
        disabled: false,
    }]
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = theme, no_json)]
struct SwitchThemeAction(SharedString);

fn menu_bar_init(cx: &mut App) -> Entity<AppMenuBar> {
    cx.on_action(|switch: &SwitchThemeAction, cx| {
        if let Some(theme_config) = ThemeRegistry::global(cx).themes().get(&switch.0).cloned() {
            Theme::global_mut(cx).apply_config(&theme_config);
        }
        cx.refresh_windows();
    });

    let menu_bar = AppMenuBar::new(cx);
    update_app_menu(menu_bar.clone(), cx);
    cx.observe_global::<Theme>({
        let menu_bar = menu_bar.clone();
        move |cx| {
            update_app_menu(menu_bar.clone(), cx);
        }
    })
    .detach();

    menu_bar
}

// Stole from gpui-component
fn embedded_themes() -> HashMap<&'static str, &'static str> {
    let mut themes = HashMap::new();

    themes.insert(
        "adventure",
        include_str!("../../../assets/builtin_assets/themes/adventure.theme"),
    );
    themes.insert(
        "alduin",
        include_str!("../../../assets/builtin_assets/themes/alduin.theme"),
    );
    themes.insert(
        "asciinema",
        include_str!("../../../assets/builtin_assets/themes/asciinema.theme"),
    );
    themes.insert(
        "ayu",
        include_str!("../../../assets/builtin_assets/themes/ayu.theme"),
    );
    themes.insert(
        "catppuccin",
        include_str!("../../../assets/builtin_assets/themes/catppuccin.theme"),
    );
    themes.insert(
        "everforest",
        include_str!("../../../assets/builtin_assets/themes/everforest.theme"),
    );
    themes.insert(
        "fahrenheit",
        include_str!("../../../assets/builtin_assets/themes/fahrenheit.theme"),
    );
    themes.insert(
        "flexoki",
        include_str!("../../../assets/builtin_assets/themes/flexoki.theme"),
    );
    themes.insert(
        "gruvbox",
        include_str!("../../../assets/builtin_assets/themes/gruvbox.theme"),
    );
    themes.insert(
        "harper",
        include_str!("../../../assets/builtin_assets/themes/harper.theme"),
    );
    themes.insert(
        "hybrid",
        include_str!("../../../assets/builtin_assets/themes/hybrid.theme"),
    );
    themes.insert(
        "jellybeans",
        include_str!("../../../assets/builtin_assets/themes/jellybeans.theme"),
    );
    themes.insert(
        "kibble",
        include_str!("../../../assets/builtin_assets/themes/kibble.theme"),
    );
    themes.insert(
        "macos-classic",
        include_str!("../../../assets/builtin_assets/themes/macos-classic.theme"),
    );
    themes.insert(
        "matrix",
        include_str!("../../../assets/builtin_assets/themes/matrix.theme"),
    );
    themes.insert(
        "mellifluous",
        include_str!("../../../assets/builtin_assets/themes/mellifluous.theme"),
    );
    themes.insert(
        "molokai",
        include_str!("../../../assets/builtin_assets/themes/molokai.theme"),
    );
    themes.insert(
        "solarized",
        include_str!("../../../assets/builtin_assets/themes/solarized.theme"),
    );
    themes.insert(
        "spaceduck",
        include_str!("../../../assets/builtin_assets/themes/spaceduck.theme"),
    );
    themes.insert(
        "tokyonight",
        include_str!("../../../assets/builtin_assets/themes/tokyonight.theme"),
    );
    themes.insert(
        "twilight",
        include_str!("../../../assets/builtin_assets/themes/twilight.theme"),
    );

    themes
}

fn load_theme(cx: &mut App) {
    let embedded = embedded_themes();
    let registry = ThemeRegistry::global_mut(cx);

    for (_, content) in embedded {
        registry.load_themes_from_str(content).unwrap();
    }
}

fn main() {
    application()
        .with_assets(gpui_component_assets::Assets)
        .run(|cx| {
            gpui_component::init(cx);
            load_theme(cx);
            cyancia_widgets::init(cx);
            cyancia_assets::init(cx);
            cyancia_shader_graph::init(cx);

            let _ = cx.open_window(
                WindowOptions {
                    titlebar: None,
                    ..Default::default()
                },
                |window, cx| {
                    let editor = cx.new(|cx| DemoEditor::new(window, cx));

                    cx.new(|cx| Root::new(editor, window, cx))
                },
            );
        });
}
