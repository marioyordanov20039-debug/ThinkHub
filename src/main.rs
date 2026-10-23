use anyhow::{bail, Context, Result};
use std::{env, path::Path, thread, time::Duration};
use thinkpad_dashboard::{
    authorization, battery, controllers, desktop, devices, firmware, inventory, plan, power, scan,
    settings, ControlRequest,
};

const HELP: &str = "ThinkPad Dashboard — hardware backend\n\nCommands:\n  scan          Read hardware and print a summary (default)\n  scan --json   Read hardware as JSON\n  status        Complete hardware, service and preferences report\n  devices       Display, input and desktop-session information\n  display brightness-plan <device> <percent> (preview only)\n  test-trackpoint Reversible, authorized one-step sensitivity test\n  battery presets | preset <name> <battery> (preview only)\n  firmware bios [--authorized] | scan | bios-plan (read-only)\n  monitor <n>   Print n JSON status samples, one second apart (1..60)\n  power show | set <profile> | test\n  settings show | save '<JSON>' | plan\n  overview      CPU, memory, graphics, drives and OS information as JSON\n  doctor        Check helper installation and competing controllers\n  capabilities  List detected controls as JSON\n  plan '<JSON>' Validate a control request without changing anything\n  apply '<JSON>' Request administrator authorization and apply a control\n  test-keyboard Request authorization, briefly change keyboard lighting, then restore it\n\nExamples:\n  thinkpad-dashboard plan '{\"kind\":\"battery_thresholds\",\"battery\":\"BAT0\",\"start\":75,\"end\":80}'\n  thinkpad-dashboard apply '{\"kind\":\"brightness\",\"device\":\"tpacpi::kbd_backlight\",\"value\":1}'\n";

fn run() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    let sys = Path::new("/sys");
    match args.first().map(String::as_str).unwrap_or("scan") {
        "-h" | "--help" | "help" => print!("{HELP}"),
        "--version" => println!("thinkpad-dashboard {}", env!("CARGO_PKG_VERSION")),
        "scan" => {
            ensure_args(&args, &["scan"], &["scan", "--json"])?;
            let snapshot = scan(sys);
            if args.get(1).map(String::as_str) == Some("--json") {
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            } else {
                print!("{}", snapshot.summary());
            }
        }
        "overview" if args.len() == 1 => println!(
            "{}",
            serde_json::to_string_pretty(&inventory::scan(
                sys,
                Path::new("/proc"),
                Path::new("/etc/os-release")
            ))?
        ),
        "status" if args.len() == 1 => println!("{}", serde_json::to_string_pretty(&status())?),
        "devices" if args.len() == 1 => println!("{}", serde_json::to_string_pretty(&serde_json::json!({"hardware":devices::scan(sys, Path::new("/run/udev/data")),"session":desktop::current()}))?),
        "display" if args.len() == 4 && args[1] == "brightness-plan" => {
            let percent: u8 = args[3].parse().context("Brightness percentage must be 0..100")?;
            let request = devices::brightness_request(sys, &args[2], percent)?;
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({"preview_only":true,"percent":percent,"plan":plan(sys, request.clone())?,"blockers":controllers::blockers(&request, &controllers::discover())}))?);
        }
        "battery" if args.len() >= 2 => match args[1].as_str() {
            "presets" if args.len() == 2 => println!("{}", serde_json::to_string_pretty(&battery::presets())?),
            "preset" if args.len() == 4 => {
                let request = battery::preset_request(&args[2], &args[3])?;
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({"preview_only":true,"plan":plan(sys, request.clone())?,"blockers":controllers::blockers(&request, &controllers::discover())}))?);
            }
            _ => bail!("Usage: battery presets | battery preset <preset> <battery> (preview only)"),
        },
        "firmware" if args.len() >= 2 => match args[1].as_str() {
            "bios" if args.len() == 2 => println!("{}", serde_json::to_string_pretty(&firmware::bios_info(sys))?),
            "bios" if args.len() == 3 && args[2] == "--authorized" => authorization::invoke("--bios-info", None)?,
            "scan" if args.len() == 2 => println!("{}", serde_json::to_string_pretty(&firmware::scan())?),
            "bios-plan" if args.len() == 2 => println!("{}",serde_json::to_string_pretty(&thinkpad_dashboard::firmware_update::inspect()?)?),
            _ => bail!("Usage: firmware bios [--authorized] | firmware scan | firmware bios-plan (read-only; no metadata refresh)"),
        },
        "monitor" if args.len() == 2 => {
            let count: u32 = args[1].parse().context("Monitor count must be 1..60")?;
            anyhow::ensure!((1..=60).contains(&count), "Monitor count must be 1..60");
            for sample in 0..count {
                println!("{}", serde_json::to_string(&status())?);
                if sample + 1 < count {
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }
        "power" if args.len() >= 2 => match args[1].as_str() {
            "show" if args.len() == 2 => {
                println!("{}", serde_json::to_string_pretty(&power::status())?)
            }
            "set" if args.len() == 3 => {
                let mut service = power::Service::connect(power::Busctl)?;
                println!("{}", serde_json::to_string_pretty(&service.set(&args[2])?)?);
            }
            "test" if args.len() == 2 => {
                let mut service = power::Service::connect(power::Busctl)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&service.reversible_test()?)?
                );
            }
            _ => bail!("Usage: power show | power set <profile> | power test"),
        },
        "settings" if args.len() >= 2 => {
            let path = settings::path()?;
            match args[1].as_str() {
                "show" if args.len() == 2 => println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"path":path,"preferences":settings::load(&path)?,"auto_apply":false})
                    )?
                ),
                "save" if args.len() == 3 => {
                    anyhow::ensure!(args[2].len() <= 65536, "Preferences exceed size limit");
                    let preferences: settings::Preferences =
                        serde_json::from_str(&args[2]).context("Invalid preferences JSON")?;
                    settings::save(&path, &preferences)?;
                    println!(
                        "{}",
                        serde_json::json!({"saved":true,"path":path,"hardware_changed":false,"auto_apply":false})
                    );
                }
                "plan" if args.len() == 2 => println!(
                    "{}",
                    serde_json::to_string_pretty(&settings_plan(&settings::load(&path)?))?
                ),
                _ => bail!("Usage: settings show | settings save '<JSON>' | settings plan"),
            }
        }
        "doctor" if args.len() == 1 => println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"installation":authorization::installation(), "controllers":controllers::discover(), "note":"Service checks cover known systemd units; manually started tools and other init systems still need dedicated detection."})
            )?
        ),
        "capabilities" if args.len() == 1 => {
            println!("{}", serde_json::to_string_pretty(&scan(sys).capabilities)?)
        }
        "plan" | "apply" if args.len() == 2 => {
            let request: ControlRequest =
                serde_json::from_str(&args[1]).context("Invalid control request JSON")?;
            let preview = plan(sys, request.clone())?;
            let blockers = controllers::blockers(&request, &controllers::discover());
            if args[0] == "plan" {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"plan":preview,"blockers":blockers})
                    )?
                );
            } else {
                if !blockers.is_empty() {
                    bail!("{}", blockers.join("\n"));
                }
                authorization::invoke("--apply", Some(&serde_json::to_string(&request)?))?;
            }
        }
        "test-keyboard" if args.len() == 1 => authorization::invoke("--test-keyboard", None)?,
        "test-trackpoint" if args.len() == 1 => authorization::invoke("--test-trackpoint", None)?,
        _ => bail!("Unknown command or arguments. Use --help."),
    }
    Ok(())
}

fn status() -> serde_json::Value {
    let preferences = settings::path().and_then(|p| settings::load(&p));
    serde_json::json!({
        "schema_version":1,"app_version":env!("CARGO_PKG_VERSION"),"hardware":scan(Path::new("/sys")),
        "overview":inventory::scan(Path::new("/sys"), Path::new("/proc"), Path::new("/etc/os-release")),
        "power":power::status(), "controllers":controllers::discover(),
        "devices":devices::scan(Path::new("/sys"), Path::new("/run/udev/data")),"desktop":desktop::current(),
        "firmware":{"client_installed":firmware::client_available(),"bios":firmware::bios_info(Path::new("/sys")),"update_scan":"Run firmware scan explicitly; status never queries fwupd"},
        "preferences":preferences.as_ref().ok(),"preferences_error":preferences.err().map(|e| format!("{e:#}"))
    })
}
fn settings_plan(preferences: &settings::Preferences) -> serde_json::Value {
    let controllers = controllers::discover();
    let batteries: Vec<_> = settings::battery_requests(preferences).into_iter().map(|request| {
        let blockers = controllers::blockers(&request, &controllers);
        match plan(Path::new("/sys"), request.clone()) {
            Ok(plan) => serde_json::json!({"plan":plan,"blockers":blockers}),
            Err(error) => serde_json::json!({"request":request,"error":format!("{error:#}"),"blockers":blockers}),
        }
    }).collect();
    let power = preferences.power_profile.as_ref().map(|desired| {
        let current = power::status();
        let advertised = current["state"]["profiles"]
            .as_array()
            .is_some_and(|profiles| profiles.iter().any(|p| p["id"].as_str() == Some(desired)));
        serde_json::json!({"desired":desired,"advertised":advertised,"current":current})
    });
    serde_json::json!({"preview_only":true,"hardware_changed":false,"batteries":batteries,"power":power,"note":"Apply individual controls explicitly. Saving does not apply settings at startup."})
}
fn ensure_args(args: &[String], a: &[&str], b: &[&str]) -> Result<()> {
    if !args.is_empty() && args != a && args != b {
        bail!("Unexpected arguments. Use --help.");
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}
