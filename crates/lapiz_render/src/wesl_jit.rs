use std::str::FromStr;

use anyhow::Result;
use wesl::{CodegenModule, CodegenPkg, ModulePath, VirtualResolver, Wesl, syntax::PathOrigin};

pub fn compile_wesl(shader: String, dependencies: &[&CodegenPkg]) -> Result<String> {
    compile_wesl_with_config(shader, dependencies, |_| {})
}

pub fn compile_wesl_with_config_and_include(
    shader: String,
    dependencies: &[&CodegenPkg],
    include: impl FnOnce(&mut VirtualResolver),
    config: impl Fn(&mut Wesl<VirtualResolver>),
) -> Result<String> {
    let mut resolver = VirtualResolver::new();
    let main_path = ModulePath::from_str("package::main").unwrap();
    resolver.add_module(main_path.clone(), shader.into());

    for pkg in dependencies {
        add_module(
            &mut resolver,
            pkg.root,
            ModulePath::new(PathOrigin::Package(pkg.root.name.to_string()), Vec::new()),
        );
    }

    include(&mut resolver);

    let mut wesl = Wesl::new_barebones().set_custom_resolver(resolver);
    wesl.set_mangler(Default::default());
    wesl.set_options(Default::default());
    config(&mut wesl);
    let shader = wesl.compile(&main_path)?;

    Ok(shader.to_string())
}

pub fn compile_wesl_with_config(
    shader: String,
    dependencies: &[&CodegenPkg],
    config: impl Fn(&mut Wesl<VirtualResolver>),
) -> Result<String> {
    compile_wesl_with_config_and_include(shader, dependencies, |_| {}, config)
}

fn add_module(resolver: &mut VirtualResolver, module: &CodegenModule, base_path: ModulePath) {
    resolver.add_module(base_path.clone(), module.source.into());

    for submodule in module.submodules {
        let mut path = base_path.clone();
        path.push(submodule.name);
        add_module(resolver, submodule, path);
    }
}
