use serde::{Deserialize, Serialize};

use crate::{SolderMaskColor, SubstrateMaterial, SurfaceFinish};

const CURRENT_MANUFACTURER_ID: &str = "jlcpcb";
const CURRENT_PROFILE_VERSION: &str = "jlcpcb-fr4-art-v2026.04";
const CURRENT_SOURCE_UPDATED_AT: &str = "2026-04-14";
const CURRENT_SOURCE_URLS: &[&str] = &[
    "https://jlcpcb.com/help/article/how-to-design-multi-color-silkscreen-using-easyeda",
    "https://jlcpcb.com/capabilities/Capabilities",
    "https://jlcpcb.com/help/article/jlcpcb-copper-weight",
    "https://jlcpcb.com/help/article/jlcpcb-surface-finish",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManufacturerProfileSnapshot {
    pub manufacturer_id: String,
    pub profile_version: String,
    pub source_updated_at: String,
    pub source_urls: Vec<String>,
    pub substrate: SubstrateMaterial,
    pub layer_count: u8,
    pub thickness_um: u32,
    pub outer_copper: CopperWeight,
    pub solder_mask: SolderMaskColor,
    pub character_process: CharacterProcess,
    pub surface_finish: SurfaceFinish,
}

impl ManufacturerProfileSnapshot {
    pub fn jlcpcb_fr4_2026_04() -> Self {
        Self {
            manufacturer_id: CURRENT_MANUFACTURER_ID.to_owned(),
            profile_version: CURRENT_PROFILE_VERSION.to_owned(),
            source_updated_at: CURRENT_SOURCE_UPDATED_AT.to_owned(),
            source_urls: CURRENT_SOURCE_URLS
                .iter()
                .map(|url| (*url).to_owned())
                .collect(),
            substrate: SubstrateMaterial::Fr4,
            layer_count: 2,
            thickness_um: 1_600,
            outer_copper: CopperWeight::Oz1,
            solder_mask: SolderMaskColor::Blue,
            character_process: CharacterProcess::StandardWhite,
            surface_finish: SurfaceFinish::Enig,
        }
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.manufacturer_id != CURRENT_MANUFACTURER_ID {
            errors.push(format!(
                "manufacturer id must be current snapshot value {CURRENT_MANUFACTURER_ID}"
            ));
        }
        if self.profile_version != CURRENT_PROFILE_VERSION {
            errors.push(format!(
                "profile version must be current snapshot value {CURRENT_PROFILE_VERSION}"
            ));
        }
        if self.source_updated_at != CURRENT_SOURCE_UPDATED_AT {
            errors.push(format!(
                "source update date must be current snapshot value {CURRENT_SOURCE_UPDATED_AT}"
            ));
        }
        if self
            .source_urls
            .iter()
            .map(String::as_str)
            .ne(CURRENT_SOURCE_URLS.iter().copied())
        {
            errors.push("source URLs must exactly match the current snapshot".to_owned());
        }
        if self.layer_count == 0 || self.thickness_um == 0 {
            errors.push("manufacturer profile dimensions must be positive".to_owned());
        }
        if !Self::supported_layer_counts().contains(&self.layer_count) {
            errors.push(format!(
                "unsupported JLCPCB FR-4 layer count: {}",
                self.layer_count
            ));
        }
        if !Self::supported_board_thicknesses_um().contains(&self.thickness_um) {
            errors.push(format!(
                "unsupported JLCPCB FR-4 board thickness: {} um",
                self.thickness_um
            ));
        }
        if !Self::supported_outer_copper_weights().contains(&self.outer_copper) {
            errors.push(format!(
                "unsupported JLCPCB FR-4 outer copper weight: {:?}",
                self.outer_copper
            ));
        }
        if !Self::supported_solder_masks().contains(&self.solder_mask) {
            errors.push(format!(
                "unsupported JLCPCB FR-4 solder mask: {:?}",
                self.solder_mask
            ));
        }
        if !Self::supported_surface_finishes().contains(&self.surface_finish) {
            errors.push(format!(
                "unsupported JLCPCB FR-4 surface finish: {:?}",
                self.surface_finish
            ));
        }
        if self.character_process == CharacterProcess::Multicolor {
            if !matches!(self.layer_count, 2 | 4) {
                errors.push("multicolor silkscreen requires 2 or 4 layers".to_owned());
            }
            if self.solder_mask != SolderMaskColor::White {
                errors.push("multicolor silkscreen requires white solder mask".to_owned());
            }
            if self.surface_finish != SurfaceFinish::Enig {
                errors.push("multicolor silkscreen requires ENIG surface finish".to_owned());
            }
            if self.outer_copper != CopperWeight::Oz1 {
                errors.push("multicolor silkscreen requires 1 oz outer copper".to_owned());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn supports_color_original_silkscreen(&self) -> bool {
        self.character_process == CharacterProcess::Multicolor && self.validate().is_ok()
    }

    pub const fn supported_layer_counts() -> &'static [u8] {
        &[
            1, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30, 32,
        ]
    }

    pub const fn supported_board_thicknesses_um() -> &'static [u32] {
        &[400, 600, 800, 1_000, 1_200, 1_600, 2_000]
    }

    pub const fn supported_outer_copper_weights() -> &'static [CopperWeight] {
        &[CopperWeight::Oz1, CopperWeight::Oz2]
    }

    pub const fn supported_solder_masks() -> &'static [SolderMaskColor] {
        &[
            SolderMaskColor::Green,
            SolderMaskColor::Red,
            SolderMaskColor::Yellow,
            SolderMaskColor::Blue,
            SolderMaskColor::White,
            SolderMaskColor::Black,
            SolderMaskColor::Purple,
        ]
    }

    pub const fn supported_surface_finishes() -> &'static [SurfaceFinish] {
        &[
            SurfaceFinish::HaslLead,
            SurfaceFinish::HaslLeadFree,
            SurfaceFinish::Enig,
        ]
    }
}

impl Default for ManufacturerProfileSnapshot {
    fn default() -> Self {
        Self::jlcpcb_fr4_2026_04()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CopperWeight {
    Oz0_5,
    Oz1,
    Oz2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CharacterProcess {
    StandardWhite,
    StandardBlack,
    Multicolor,
}
