//! Non-privileged-by-design commands (some still require sudo because the
//! daemon enforces it).

use crate::cli::{BrowserAction, CheckArgs, ConfigAction, StatsAction, StatsArgs};
use crate::context::{Ctx, system_dns_routing};
use crate::output::{self, DnsRouting};
use anyhow::{Context, Result, anyhow, bail};
use fuck_ads_config::DEFAULT_CONFIG_TOML;
use fuck_ads_core::control::ControlError;
use fuck_ads_core::protocol::{Request, Response};
use fuck_ads_filter::ResourceType;
use fuck_ads_platform::{ServiceStatus, browser};
use std::process::ExitCode;

fn unexpected(r: Response) -> anyhow::Error {
    anyhow!("unexpected response from the service: {r:?}")
}

pub async fn status(ctx: &Ctx) -> Result<ExitCode> {
    match ctx.request(Request::Status).await {
        Ok(Response::Status(s)) => {
            let routing = if s.dns_enabled {
                system_dns_routing().await
            } else {
                DnsRouting::Unknown
            };
            if ctx.json {
                let mut v = serde_json::to_value(&*s)?;
                v["system_dns_routed"] = serde_json::json!(routing == DnsRouting::Active);
                ctx.print_json(&v)?;
            } else {
                print!("{}", output::render_status(&s, routing, ctx.style));
            }
            Ok(ExitCode::SUCCESS)
        }
        Ok(Response::Error { message }) => bail!(message),
        Ok(other) => Err(unexpected(other)),
        Err(ControlError::NotRunning(_)) | Err(ControlError::Timeout) => {
            let installed =
                fuck_ads_platform::current().service_status() != ServiceStatus::NotInstalled;
            if ctx.json {
                ctx.print_json(&serde_json::json!({ "running": false, "installed": installed }))?;
            } else {
                print!("{}", output::render_not_running(ctx.style, installed));
            }
            // Like `systemctl status`: 3 = not running.
            Ok(ExitCode::from(3))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn stats(ctx: &Ctx, args: &StatsArgs) -> Result<()> {
    if let Some(StatsAction::Reset) = args.action {
        return simple(ctx, Request::StatsReset).await;
    }
    match ctx
        .call(Request::Stats {
            days: Some(args.days),
        })
        .await?
    {
        Response::Stats(s) if ctx.json => ctx.print_json(&s),
        Response::Stats(s) => {
            print!("{}", output::render_stats(&s, ctx.style));
            Ok(())
        }
        other => Err(unexpected(other)),
    }
}

pub async fn rules(ctx: &Ctx) -> Result<()> {
    match ctx.call(Request::Rules).await? {
        Response::Rules(lists) if ctx.json => ctx.print_json(&lists),
        Response::Rules(lists) => {
            print!(
                "{}",
                output::render_rules(&lists, ctx.style, fuck_ads_stats::time::now_unix())
            );
            Ok(())
        }
        other => Err(unexpected(other)),
    }
}

fn parse_resource_type(s: &str) -> Result<ResourceType> {
    let normalized = match s {
        "main_frame" => "document",
        "sub_frame" => "subdocument",
        other => other,
    };
    ResourceType::from_option(normalized).ok_or_else(|| anyhow!("unknown resource type {s:?}"))
}

pub async fn check(ctx: &Ctx, args: &CheckArgs) -> Result<()> {
    let req = if args.target.contains("://") {
        Request::CheckUrl {
            url: args.target.clone(),
            source: args.source.clone(),
            resource_type: args
                .r#type
                .as_deref()
                .map(parse_resource_type)
                .transpose()?,
        }
    } else {
        Request::Check {
            domain: args.target.clone(),
        }
    };
    match ctx.call(req).await? {
        Response::Check(c) if ctx.json => ctx.print_json(&c),
        Response::Check(c) => {
            print!("{}", output::render_check(&c, ctx.style));
            Ok(())
        }
        other => Err(unexpected(other)),
    }
}

/// Sends a request whose success response is a message.
pub async fn simple(ctx: &Ctx, req: Request) -> Result<()> {
    match ctx.call(req).await {
        Ok(Response::Ok { message }) => {
            if ctx.json {
                ctx.print_json(&serde_json::json!({ "ok": true, "message": message }))
            } else {
                println!("{} {message}", ctx.style.green("✓"));
                Ok(())
            }
        }
        Ok(other) => Err(unexpected(other)),
        Err(e) if e.to_string().contains("permission denied") => {
            bail!(
                "{e}\n\nThis changes system-wide settings. Try:\n\n    sudo fuck-ads {}",
                std::env::args().skip(1).collect::<Vec<_>>().join(" ")
            )
        }
        Err(e) => Err(e),
    }
}

pub async fn list(ctx: &Ctx, req: Request, title: &str) -> Result<()> {
    match ctx.call(req).await? {
        Response::List(items) if ctx.json => ctx.print_json(&items),
        Response::List(items) => {
            if items.is_empty() {
                println!("{title}: (empty)");
            } else {
                for item in items {
                    println!("{item}");
                }
            }
            Ok(())
        }
        other => Err(unexpected(other)),
    }
}

pub async fn update(ctx: &Ctx, force: bool) -> Result<()> {
    if !ctx.json {
        println!("Updating filter lists…");
    }
    simple(ctx, Request::Update { force }).await
}

pub fn config(ctx: &Ctx, action: Option<&ConfigAction>) -> Result<()> {
    match action.unwrap_or(&ConfigAction::Show) {
        ConfigAction::Path => println!("{}", ctx.paths.config_file.display()),
        ConfigAction::Default => print!("{DEFAULT_CONFIG_TOML}"),
        ConfigAction::Validate => {
            let exists = ctx.paths.config_file.exists();
            ctx.config()?;
            if exists {
                println!(
                    "{} {} is valid",
                    ctx.style.green("✓"),
                    ctx.paths.config_file.display()
                );
            } else {
                println!(
                    "{} not found; built-in defaults are in use",
                    ctx.paths.config_file.display()
                );
            }
        }
        ConfigAction::Show => {
            let cfg = ctx.config()?;
            if ctx.json {
                ctx.print_json(&cfg)?;
            } else {
                println!("# {}", ctx.paths.config_file.display());
                print!("{}", cfg.to_toml());
            }
        }
    }
    Ok(())
}

fn native_host_path() -> Result<std::path::PathBuf> {
    let exe = std::env::current_exe().context("cannot locate the fuck-ads executable")?;
    let host = exe.with_file_name("fuck-ads-native-host");
    if host.exists() {
        return Ok(host);
    }
    // Development builds: create the symlink next to the binary.
    std::os::unix::fs::symlink(&exe, &host)
        .with_context(|| format!("cannot create {}; reinstall FuckAds", host.display()))?;
    Ok(host)
}

fn home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(Into::into)
}

pub async fn browser(ctx: &Ctx, action: &BrowserAction) -> Result<()> {
    match action {
        BrowserAction::Install {
            system,
            extension_ids,
        } => {
            let host = native_host_path()?;
            let written = if *system {
                if !fuck_ads_platform::is_root() {
                    bail!(
                        "system-wide registration requires root: sudo fuck-ads browser install --system"
                    );
                }
                browser::install(&browser::system_locations(), &host, extension_ids, false)?
            } else {
                let home = home().context("HOME is not set")?;
                browser::install(&browser::user_locations(&home), &host, extension_ids, true)?
            };
            if written.is_empty() {
                println!(
                    "No supported browsers found. Install Chrome, Chromium, Brave, Edge, Vivaldi or Firefox first."
                );
            }
            for p in written {
                println!("{} registered {}", ctx.style.green("✓"), p.display());
            }
            println!("\nNext: load the FuckAds extension in your browser (see docs/browser.md).");
        }
        BrowserAction::Uninstall { system } => {
            let locations = if *system {
                browser::system_locations()
            } else {
                browser::user_locations(&home().context("HOME is not set")?)
            };
            for p in browser::uninstall(&locations) {
                println!("removed {}", p.display());
            }
        }
        BrowserAction::Status => {
            let installed = browser::installed(home().as_deref());
            if installed.is_empty() {
                println!(
                    "{} native messaging host not registered (run: fuck-ads browser install)",
                    ctx.style.red("✗")
                );
            } else {
                for (b, p) in &installed {
                    println!("{} {b}: {}", ctx.style.green("✓"), p.display());
                }
            }
            if let Ok(Response::Status(s)) = ctx.request(Request::Status).await {
                let (state, label) = output::browser_state(&s);
                println!("{} extension: {label}", ctx.style.check(state));
            }
        }
        BrowserAction::ExportSafari { output } => match ctx.call(Request::SafariRules).await? {
            Response::Safari { rules, .. } => {
                let bytes = serde_json::to_vec(&rules)?;
                std::fs::write(output, bytes)
                    .with_context(|| format!("cannot write {}", output.display()))?;
                println!(
                    "{} wrote {} Safari content-blocker rules to {}",
                    ctx.style.green("✓"),
                    rules.len(),
                    output.display()
                );
            }
            other => return Err(unexpected(other)),
        },
    }
    Ok(())
}
