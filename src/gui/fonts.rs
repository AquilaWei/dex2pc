use crate::runner::{CommandRunner, SystemRunner};
use eframe::egui;
use std::io::{self, Write};

pub(super) const NOTICE: &str = "CJK font missing. Install google-noto-sans-cjk-fonts (Fedora) or fonts-noto-cjk (Debian/Ubuntu), then restart.";
const FALLBACKS: &[&str] = &[
    "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-sans-cjk-vf-fonts/NotoSansCJK-VF.ttc",
    "/usr/share/fonts/google-noto-sans-cjk-vf-fonts/NotoSansTC[wght].otf",
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
];

// Fontconfig may return a Latin font when no CJK font exists. Validate coverage
// before handing bytes to egui, which otherwise panics on invalid font data.
fn face_index(bytes: &[u8]) -> Option<u32> {
    let mut first = None;
    for index in 0..ttf_parser::fonts_in_collection(bytes).unwrap_or(1) {
        let Ok(face) = ttf_parser::Face::parse(bytes, index) else {
            continue;
        };
        if face.glyph_index('中').is_none() || face.glyph_index('體').is_none() {
            continue;
        }
        first.get_or_insert(index);
        if face.names().into_iter().any(|name| {
            name.name_id == ttf_parser::name_id::FAMILY
                && name.to_string().is_some_and(|name| {
                    name.contains("CJK TC") || name.contains("Sans TC") || name.contains("SansTC")
                })
        }) {
            return Some(index);
        }
    }
    first
}

fn find_font<T>(
    runner: &dyn CommandRunner,
    fallbacks: &[&str],
    mut load: impl FnMut(&str) -> Option<T>,
) -> Option<T> {
    if let Ok(output) = runner.run("fc-match", &["-f", "%{file}", "sans-serif:lang=zh-tw"])
        && output.success()
        && let Some(font) = load(output.stdout.trim())
    {
        return Some(font);
    }
    fallbacks.iter().find_map(|path| load(path))
}

fn append(fonts: &mut egui::FontDefinitions, font: egui::FontData) {
    fonts.font_data.insert("cjk".into(), font.into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("cjk".into());
    }
}

fn definitions(
    font: Option<egui::FontData>,
    warning: &mut dyn Write,
) -> (egui::FontDefinitions, bool) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(font) = font {
        append(&mut fonts, font);
        (fonts, true)
    } else {
        let _ = writeln!(warning, "dex2pc: warning: {NOTICE}");
        (fonts, false)
    }
}

pub(super) fn install(ctx: &egui::Context) -> bool {
    let font = find_font(&SystemRunner, FALLBACKS, |path| {
        let bytes = std::fs::read(path).ok()?;
        let index = face_index(&bytes)?;
        let mut font = egui::FontData::from_owned(bytes);
        font.index = index;
        Some(font)
    });
    let (fonts, found) = definitions(font, &mut io::stderr());
    ctx.set_fonts(fonts);
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::CommandOutput;

    struct Lookup(bool);
    impl CommandRunner for Lookup {
        fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
            assert_eq!(program, "fc-match");
            assert_eq!(args, ["-f", "%{file}", "sans-serif:lang=zh-tw"]);
            if !self.0 {
                return Err(io::Error::other("fontconfig unavailable"));
            }
            Ok(CommandOutput {
                code: Some(0),
                stdout: "/chosen.tc.ttc".into(),
                stderr: String::new(),
            })
        }
    }

    #[test]
    fn fontconfig_is_preferred_over_fallback_paths() {
        assert_eq!(
            find_font(&Lookup(true), &["/fallback.ttc"], |path| Some(
                path.to_owned()
            )),
            Some("/chosen.tc.ttc".into())
        );
    }

    #[test]
    fn fallback_is_used_when_fontconfig_fails() {
        assert_eq!(
            find_font(&Lookup(false), &["/fallback.ttc"], |path| Some(
                path.to_owned()
            )),
            Some("/fallback.ttc".into())
        );
    }

    #[test]
    fn missing_font_warns_without_panicking() {
        let mut warning = Vec::new();
        let font = find_font(&Lookup(false), &[], |_| None::<egui::FontData>);
        definitions(font, &mut warning);
        assert_eq!(
            String::from_utf8(warning).unwrap(),
            "dex2pc: warning: CJK font missing. Install google-noto-sans-cjk-fonts (Fedora) or fonts-noto-cjk (Debian/Ubuntu), then restart.\n"
        );
    }

    #[test]
    fn proportional_fallback_is_after_default_fonts() {
        let (fonts, _) = definitions(Some(egui::FontData::from_owned(vec![])), &mut Vec::new());
        assert_eq!(
            fonts.families[&egui::FontFamily::Proportional],
            [
                "Ubuntu-Light",
                "NotoEmoji-Regular",
                "emoji-icon-font",
                "cjk"
            ]
        );
    }

    #[test]
    fn monospace_fallback_is_after_default_fonts() {
        let (fonts, _) = definitions(Some(egui::FontData::from_owned(vec![])), &mut Vec::new());
        assert_eq!(
            fonts.families[&egui::FontFamily::Monospace],
            [
                "Hack",
                "Ubuntu-Light",
                "NotoEmoji-Regular",
                "emoji-icon-font",
                "cjk"
            ]
        );
    }

    #[test]
    fn invalid_font_is_rejected() {
        assert_eq!(face_index(b"invalid"), None);
    }
}
