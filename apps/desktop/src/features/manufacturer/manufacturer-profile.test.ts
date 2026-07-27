import { describe, expect, it } from "vitest";

import type { ManufacturerProfileSnapshot } from "@/lib/core";
import {
  getManufacturerPalette,
  manufacturerGeometrySignature,
  resolveBoardMaterial,
  validateManufacturerProfile,
} from "@/features/manufacturer/manufacturer-profile";

const profile: ManufacturerProfileSnapshot = {
  manufacturerId: "jlcpcb",
  profileVersion: "jlcpcb-fr4-art-v2026.04",
  sourceUpdatedAt: "2026-04-14",
  sourceUrls: [],
  substrate: "fr4",
  layerCount: 2,
  thicknessUm: 1_600,
  outerCopper: "oz1",
  solderMask: "white",
  characterProcess: "standardBlack",
  surfaceFinish: "enig",
};

describe("manufacturer profile UI contract", () => {
  it("rejects OSP, 0.5 oz, and invalid multicolor combinations for FR-4", () => {
    expect(
      validateManufacturerProfile({ ...profile, surfaceFinish: "osp" }),
    ).toContain("当前嘉立创 FR-4 能力仅允许喷锡或沉金；OSP 仅用于铜基板");
    expect(
      validateManufacturerProfile({ ...profile, outerCopper: "oz0_5" }),
    ).toContain("当前嘉立创 FR-4 不支持 0.5 oz 外层铜");
    expect(
      validateManufacturerProfile({
        ...profile,
        characterProcess: "multicolor",
        solderMask: "purple",
      }),
    ).toContain("彩色丝印要求白色阻焊");
    expect(
      validateManufacturerProfile({
        ...profile,
        characterProcess: "multicolor",
        layerCount: 6,
      }),
    ).toContain("彩色丝印只支持 2 层或 4 层板");
    expect(
      validateManufacturerProfile({
        ...profile,
        characterProcess: "multicolor",
        outerCopper: "oz2",
      }),
    ).toContain("彩色丝印要求 1 oz 外层铜");
    expect(
      validateManufacturerProfile({
        ...profile,
        characterProcess: "multicolor",
        surfaceFinish: "haslLeadFree",
      }),
    ).toContain("彩色丝印要求沉金（ENIG）表面处理");
  });

  it("derives white, JLCPCB purple, ENIG and HASL approximations", () => {
    expect(getManufacturerPalette(profile)).toMatchObject({
      substrate: "#b0844f",
      solderMask: "#e2e4de",
      exposedCopper: "#d3a639",
      silkscreen: "#181817",
    });

    expect(
      getManufacturerPalette({
        ...profile,
        solderMask: "purple",
        surfaceFinish: "haslLeadFree",
      }),
    ).toMatchObject({
      solderMask: "#5b2f70",
      exposedCopper: "#c3c7c9",
    });
  });

  it("changes material color without changing the production geometry signature", () => {
    const enig = getManufacturerPalette(profile);
    const haslProfile: ManufacturerProfileSnapshot = {
      ...profile,
      surfaceFinish: "haslLeadFree",
    };
    const hasl = getManufacturerPalette(haslProfile);

    expect(enig.exposedCopper).not.toBe(hasl.exposedCopper);
    expect(manufacturerGeometrySignature(profile)).toBe(
      manufacturerGeometrySignature(haslProfile),
    );
    expect(manufacturerGeometrySignature(profile)).toBe(
      manufacturerGeometrySignature({
        ...profile,
        solderMask: "purple",
        characterProcess: "standardWhite",
      }),
    );
    expect(resolveBoardMaterial(enig, { solderMaskOpen: true, hasCopper: true }))
      .toBe(enig.exposedCopper);
    expect(resolveBoardMaterial(enig, { solderMaskOpen: true, hasCopper: false }))
      .toBe(enig.substrate);
    expect(resolveBoardMaterial(enig, { solderMaskOpen: false, hasCopper: true }))
      .toBe(enig.solderMask);
  });
});
