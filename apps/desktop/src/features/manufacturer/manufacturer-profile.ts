import type {
  ManufacturerProfileSnapshot,
  SolderMaskColor,
  StackupPreset,
} from "@/lib/core";

export interface ManufacturerPalette {
  substrate: string;
  exposedCopper: string;
  solderMask: string;
  silkscreen: string;
}

const SOLDER_MASK_COLORS: Record<SolderMaskColor, string> = {
  black: "#1b1d1c",
  white: "#e2e4de",
  green: "#146941",
  red: "#992421",
  blue: "#234387",
  purple: "#5b2f70",
  yellow: "#d7ab25",
};

export function validateManufacturerProfile(
  profile: ManufacturerProfileSnapshot,
): string[] {
  const errors: string[] = [];

  if (profile.substrate !== "fr4") {
    errors.push("当前制造能力配置仅支持 FR-4 基材");
  }
  if (
    ![
      1, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30, 32,
    ].includes(profile.layerCount)
  ) {
    errors.push("当前嘉立创 FR-4 不支持所选层数");
  }
  if (
    ![400, 600, 800, 1_000, 1_200, 1_600, 2_000].includes(
      profile.thicknessUm,
    )
  ) {
    errors.push("当前嘉立创 FR-4 不支持所选板厚");
  }
  if (profile.outerCopper === "oz0_5") {
    errors.push("当前嘉立创 FR-4 不支持 0.5 oz 外层铜");
  }
  if (profile.surfaceFinish === "osp") {
    errors.push("当前嘉立创 FR-4 能力仅允许喷锡或沉金；OSP 仅用于铜基板");
  }
  if (profile.characterProcess === "multicolor") {
    if (profile.layerCount !== 2 && profile.layerCount !== 4) {
      errors.push("彩色丝印只支持 2 层或 4 层板");
    }
    if (profile.solderMask !== "white") {
      errors.push("彩色丝印要求白色阻焊");
    }
    if (profile.surfaceFinish !== "enig") {
      errors.push("彩色丝印要求沉金（ENIG）表面处理");
    }
    if (profile.outerCopper !== "oz1") {
      errors.push("彩色丝印要求 1 oz 外层铜");
    }
  }

  return errors;
}

export function getManufacturerPalette(
  profile: ManufacturerProfileSnapshot,
): ManufacturerPalette {
  return {
    substrate: "#b0844f",
    exposedCopper:
      profile.surfaceFinish === "enig" ? "#d3a639" : "#c3c7c9",
    solderMask: SOLDER_MASK_COLORS[profile.solderMask],
    silkscreen:
      profile.characterProcess === "standardBlack" ? "#181817" : "#f8f6e0",
  };
}

/**
 * Returns the part of the profile that may affect board geometry.
 * Solder mask, characters and surface finish deliberately stay out of this
 * signature so palette-only edits can reuse compiled production masks.
 */
export function manufacturerGeometrySignature(
  profile: ManufacturerProfileSnapshot,
): string {
  return [
    profile.substrate,
    profile.layerCount,
    profile.thicknessUm,
    profile.outerCopper,
  ].join(":");
}

export function manufacturerProfileToStackup(
  profile: ManufacturerProfileSnapshot,
): StackupPreset {
  return {
    substrate: profile.substrate,
    thicknessUm: profile.thicknessUm,
    solderMaskColor: profile.solderMask,
    surfaceFinish: profile.surfaceFinish,
  };
}

export function resolveBoardMaterial(
  palette: ManufacturerPalette,
  state: {
    hasCopper: boolean;
    solderMaskOpen: boolean;
    hasSilkscreen?: boolean;
  },
): string {
  if (state.hasSilkscreen) return palette.silkscreen;
  if (!state.solderMaskOpen) return palette.solderMask;
  return state.hasCopper ? palette.exposedCopper : palette.substrate;
}
