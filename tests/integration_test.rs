use std::fs;
use std::process::Command;
use std::path::Path;

#[test]
fn test_compile_and_run() -> Result<(), Box<dyn std::error::Error>> {
    let paths = fs::read_dir("tests/sk/")?;
    for item in paths {
        run_sk_test(&item.unwrap().path())?;
    }
    Ok(())
}

/// Execute tests/sk/x.sk
/// Fail if it prints something
fn run_sk_test(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        dbg!(&path);
    let src = fs::read_to_string(path)?;

    let ast = shiika::parser::Parser::parse(&src)?;
    let corelib = shiika::corelib::Corelib::create();
    let hir = shiika::hir::build(ast, corelib)?;
    let mut code_gen = shiika::code_gen::CodeGen::new(&hir);
    code_gen.gen_program(&hir)?;
    code_gen.module.print_to_file("tests/out.ll")?;

    let mut cmd = Command::new("llc");
    cmd.arg("tests/out.ll");
    cmd.output().unwrap();

    let mut cmd = Command::new("cc");
    cmd.arg("-I/usr/local/Cellar/bdw-gc/7.6.0/include/");
    cmd.arg("-L/usr/local/Cellar/bdw-gc/7.6.0/lib/");
    cmd.arg("-lgc");
    cmd.arg("-otests/out");
    cmd.arg("tests/out.s");
    cmd.output().unwrap();

    let mut cmd = Command::new("tests/out");
    let output = cmd.output().expect("failed to execute process");
    let stdout = String::from_utf8(output.stdout).expect("invalid utf8 in stdout");
    let stderr = String::from_utf8(output.stderr).expect("invalid utf8 in stderr");
    assert_eq!(stderr, "");
    assert_eq!(stdout, "");
    Ok(())
}

