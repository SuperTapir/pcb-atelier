import { useState } from "react";

import {
  getManufacturerPalette,
  manufacturerGeometrySignature,
  validateManufacturerProfile,
} from "@/features/manufacturer/manufacturer-profile";
import type { ManufacturerProfileSnapshot } from "@/lib/core";

export interface ManufacturerInspectorProps {
  profile: ManufacturerProfileSnapshot;
  onChange: (profile: ManufacturerProfileSnapshot) => void;
  onRejected?: (message: string) => void;
}

export function ManufacturerInspector({
  onChange,
  onRejected,
  profile,
}: ManufacturerInspectorProps) {
  const [validationMessage, setValidationMessage] = useState<string | null>(
    null,
  );
  const palette = getManufacturerPalette(profile);

  const commit = (patch: Partial<ManufacturerProfileSnapshot>) => {
    const candidate = { ...profile, ...patch };
    const errors = validateManufacturerProfile(candidate);
    if (errors.length > 0) {
      const message = errors.join("；");
      setValidationMessage(message);
      onRejected?.(message);
      return;
    }
    setValidationMessage(null);
    onChange(candidate);
  };

  return (
    <section
      aria-label="板面工艺"
      className="space-y-4"
      data-geometry-signature={manufacturerGeometrySignature(profile)}
    >
      <header>
        <h3 className="text-xs font-semibold">板面工艺</h3>
        <p className="mt-1 text-[10px] leading-4 text-muted-foreground">
          选择阻焊颜色与露铜区域的表面处理。
        </p>
      </header>

      <div className="space-y-3">
        <ManufacturingSelect
          label="阻焊油墨"
          onChange={(value) =>
            commit({
              solderMask: value as ManufacturerProfileSnapshot["solderMask"],
            })
          }
          options={[
            ["green", "绿色"],
            ["red", "红色"],
            ["yellow", "黄色"],
            ["blue", "蓝色"],
            ["white", "白色"],
            ["black", "哑黑"],
            ["purple", "嘉立创紫"],
          ]}
          swatch={palette.solderMask}
          value={profile.solderMask}
        />
        <ManufacturingSelect
          label="露铜表面处理"
          onChange={(value) =>
            commit({
              surfaceFinish:
                value as ManufacturerProfileSnapshot["surfaceFinish"],
            })
          }
          options={[
            ["enig", "沉金（ENIG）"],
            ["haslLead", "有铅喷锡"],
            ["haslLeadFree", "无铅喷锡"],
            ["osp", "OSP（仅铜基板，FR-4 不支持）", true],
          ]}
          swatch={palette.exposedCopper}
          value={profile.surfaceFinish}
        />
      </div>

      <div
        className="grid grid-cols-2 gap-2 rounded-lg border bg-background/50 p-2.5 text-[9px] text-muted-foreground"
        data-testid="material-semantics"
      >
        <MaterialLegend color={palette.substrate} label="无铜开窗 · FR-4" />
        <MaterialLegend
          color={palette.exposedCopper}
          label="有铜开窗 · 表面处理"
        />
      </div>

      {validationMessage && (
        <p
          className="rounded-md border border-red-500/30 bg-red-500/10 p-2 text-[10px] leading-4 text-foreground"
          role="alert"
        >
          {validationMessage}
        </p>
      )}
      <p className="text-[9px] leading-4 text-muted-foreground">
        屏幕近似色不代表批次实物色差保证。修改阻焊或表面处理只更新显示材质，不重建生产几何。
      </p>
    </section>
  );
}

function ManufacturingSelect({
  label,
  onChange,
  options,
  swatch,
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  options: Array<[string, string, boolean?]>;
  swatch: string;
  value: string;
}) {
  return (
    <label className="grid grid-cols-[92px_minmax(0,1fr)] items-center gap-3 text-[10px]">
      <span className="flex items-center gap-2 text-muted-foreground">
        <span
          aria-hidden="true"
          className="size-3 rounded-full border shadow-sm"
          style={{ backgroundColor: swatch }}
        />
        {label}
      </span>
      <select
        aria-label={label}
        className="h-8 min-w-0 rounded-md border bg-background px-2 text-[11px]"
        onChange={(event) => onChange(event.currentTarget.value)}
        value={value}
      >
        {options.map(([optionValue, optionLabel, disabled]) => (
          <option disabled={disabled} key={optionValue} value={optionValue}>
            {optionLabel}
          </option>
        ))}
      </select>
    </label>
  );
}

function MaterialLegend({ color, label }: { color: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <span
        aria-hidden="true"
        className="size-3 rounded-sm border"
        style={{ backgroundColor: color }}
      />
      {label}
    </span>
  );
}
