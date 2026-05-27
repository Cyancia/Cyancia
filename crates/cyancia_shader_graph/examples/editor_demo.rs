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
    ActiveTheme, GlobalState, Root, Theme, ThemeMode, ThemeRegistry, TitleBar, menu::AppMenuBar,
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
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut nodes = builtin_nodes();
        nodes.register::<GraphInputNode>();
        nodes.register::<GraphOutputNode>();
        Self {
            menu_bar: menu_bar_init(cx),
            editor: cx.new(|cx| {
                GraphEditor::new(
                    Graph::new(GraphResources::default().into(), builtin_types().into()),
                    nodes.into(),
                    cx,
                )
            }),
        }
    }
}

impl Render for DemoEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

    themes.insert("adventure", include_str!("themes/adventure.json"));
    themes.insert("alduin", include_str!("themes/alduin.json"));
    themes.insert("asciinema", include_str!("themes/asciinema.json"));
    themes.insert("ayu", include_str!("themes/ayu.json"));
    themes.insert("catppuccin", include_str!("themes/catppuccin.json"));
    themes.insert("everforest", include_str!("themes/everforest.json"));
    themes.insert("fahrenheit", include_str!("themes/fahrenheit.json"));
    themes.insert("flexoki", include_str!("themes/flexoki.json"));
    themes.insert("gruvbox", include_str!("themes/gruvbox.json"));
    themes.insert("harper", include_str!("themes/harper.json"));
    themes.insert("hybrid", include_str!("themes/hybrid.json"));
    themes.insert("jellybeans", include_str!("themes/jellybeans.json"));
    themes.insert("kibble", include_str!("themes/kibble.json"));
    themes.insert("macos-classic", include_str!("themes/macos-classic.json"));
    themes.insert("matrix", include_str!("themes/matrix.json"));
    themes.insert("mellifluous", include_str!("themes/mellifluous.json"));
    themes.insert("molokai", include_str!("themes/molokai.json"));
    themes.insert("solarized", include_str!("themes/solarized.json"));
    themes.insert("spaceduck", include_str!("themes/spaceduck.json"));
    themes.insert("tokyonight", include_str!("themes/tokyonight.json"));
    themes.insert("twilight", include_str!("themes/twilight.json"));

    themes
}

fn load_theme(cx: &mut App) {
    let embedded = embedded_themes();
    let registry = ThemeRegistry::global_mut(cx);

    for (name, content) in embedded {
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

            cx.open_window(
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
