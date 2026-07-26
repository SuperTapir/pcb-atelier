import { OrbitControls } from "@react-three/drei";
import { Canvas } from "@react-three/fiber";
import { useEffect, useMemo } from "react";
import {
  DataTexture,
  DoubleSide,
  ExtrudeGeometry,
  LinearFilter,
  MeshStandardMaterial,
  RGBAFormat,
  Shape,
  ShapeGeometry,
  SRGBColorSpace,
  UnsignedByteType,
} from "three";

import {
  getBoardPreviewGeometry,
  validateBoardPreviewInput,
  type BoardPreviewInput,
  type BoardPreviewRenderer,
  type BoardPreviewRendererProps,
  type BoardPreviewTexture,
} from "@/features/preview/board-preview-renderer";

const FACE_OFFSET_MM = 0.01;

export function Board3DPreview({
  preview,
  className,
}: BoardPreviewRendererProps) {
  const errors = validateBoardPreviewInput(preview);
  const geometry = getBoardPreviewGeometry(preview);
  const longestSide = Math.max(geometry.widthMm, geometry.heightMm);

  return (
    <div
      aria-label="3D 成板效果预览"
      className={className}
      data-testid="board-3d-preview"
      role="img"
      style={{
        minHeight: 240,
        overflow: "hidden",
        position: "relative",
        width: "100%",
      }}
    >
      {errors.length > 0 ? (
        <div
          data-testid="board-3d-preview-error"
          role="status"
          style={{ padding: 16 }}
        >
          无法显示成板预览：{errors.join("；")}
        </div>
      ) : (
        <Canvas
          camera={{
            far: longestSide * 20,
            fov: 38,
            near: Math.max(0.01, longestSide / 1_000),
            position: [
              longestSide * 0.35,
              -longestSide * 0.65,
              longestSide * 1.15,
            ],
          }}
          dpr={[1, 2]}
          gl={{ alpha: true, antialias: true }}
        >
          <color args={["#e9e6df"]} attach="background" />
          <ambientLight intensity={1.5} />
          <directionalLight
            intensity={2.1}
            position={[
              longestSide * 0.6,
              -longestSide * 0.4,
              longestSide * 1.5,
            ]}
          />
          <directionalLight
            intensity={0.65}
            position={[
              -longestSide * 0.8,
              longestSide * 0.8,
              -longestSide,
            ]}
          />
          <BoardMesh preview={preview} />
          <OrbitControls
            enableDamping
            enablePan
            enableRotate
            enableZoom
            makeDefault
            maxDistance={longestSide * 5}
            minDistance={longestSide * 0.45}
            panSpeed={0.7}
            rotateSpeed={0.65}
            zoomSpeed={0.8}
          />
        </Canvas>
      )}
      <div
        aria-hidden="true"
        style={{
          bottom: 12,
          color: "rgba(31, 29, 25, 0.62)",
          fontSize: 12,
          left: 0,
          pointerEvents: "none",
          position: "absolute",
          right: 0,
          textAlign: "center",
        }}
      >
        拖动旋转 · 滚轮缩放 · 右键平移
      </div>
    </div>
  );
}

export const board3DPreviewRenderer: BoardPreviewRenderer = {
  id: "three-board-preview",
  Component: Board3DPreview,
};

function BoardMesh({ preview }: { preview: BoardPreviewInput }) {
  const dimensions = getBoardPreviewGeometry(preview);
  const shape = useMemo(
    () =>
      roundedRectangleShape(
        dimensions.widthMm,
        dimensions.heightMm,
        dimensions.cornerRadiusMm,
      ),
    [
      dimensions.cornerRadiusMm,
      dimensions.heightMm,
      dimensions.widthMm,
    ],
  );
  const bodyGeometry = useMemo(
    () =>
      new ExtrudeGeometry(shape, {
        bevelEnabled: false,
        curveSegments: 12,
        depth: dimensions.thicknessMm,
        steps: 1,
      }),
    [dimensions.thicknessMm, shape],
  );
  const faceGeometry = useMemo(() => {
    const result = new ShapeGeometry(shape, 12);
    normalizeFaceUvs(result, dimensions.widthMm, dimensions.heightMm);
    return result;
  }, [dimensions.heightMm, dimensions.widthMm, shape]);
  const frontTexture = useBoardTexture(preview.textures.front);
  const backTexture = useBoardTexture(preview.textures.back);
  const bodyMaterial = useMemo(
    () =>
      new MeshStandardMaterial({
        color: "#174a3a",
        metalness: 0.08,
        roughness: 0.58,
      }),
    [],
  );
  const frontMaterial = useMemo(
    () =>
      new MeshStandardMaterial({
        map: frontTexture,
        metalness: 0.12,
        roughness: 0.56,
        side: DoubleSide,
      }),
    [frontTexture],
  );
  const backMaterial = useMemo(
    () =>
      new MeshStandardMaterial({
        map: backTexture,
        metalness: 0.12,
        roughness: 0.56,
        side: DoubleSide,
      }),
    [backTexture],
  );

  useEffect(
    () => () => {
      bodyGeometry.dispose();
      faceGeometry.dispose();
      bodyMaterial.dispose();
      frontMaterial.dispose();
      backMaterial.dispose();
    },
    [
      backMaterial,
      bodyGeometry,
      bodyMaterial,
      faceGeometry,
      frontMaterial,
    ],
  );

  const frontZ = dimensions.thicknessMm + FACE_OFFSET_MM;
  const backZ = -FACE_OFFSET_MM;

  return (
    <group rotation={[-0.05, 0, 0]}>
      <mesh geometry={bodyGeometry} material={bodyMaterial} />
      <mesh
        geometry={faceGeometry}
        material={frontMaterial}
        position={[0, 0, frontZ]}
      />
      {/*
       * The back texture remains in physical board coordinates. Rotating the
       * physical back face around its centre supplies the viewing mirror;
       * source pixels and export direction are never rewritten.
       */}
      <mesh
        geometry={faceGeometry}
        material={backMaterial}
        position={[0, 0, backZ]}
        rotation={[0, Math.PI, 0]}
      />
    </group>
  );
}

function useBoardTexture(texture: BoardPreviewTexture) {
  const dataTexture = useMemo(() => {
    const result = new DataTexture(
      Uint8Array.from(texture.rgba),
      texture.widthPx,
      texture.heightPx,
      RGBAFormat,
      UnsignedByteType,
    );
    result.colorSpace = SRGBColorSpace;
    result.flipY = true;
    result.magFilter = LinearFilter;
    result.minFilter = LinearFilter;
    result.generateMipmaps = false;
    result.needsUpdate = true;
    return result;
  }, [texture.heightPx, texture.rgba, texture.widthPx]);

  useEffect(() => () => dataTexture.dispose(), [dataTexture]);
  return dataTexture;
}

function roundedRectangleShape(
  width: number,
  height: number,
  radius: number,
) {
  const halfWidth = width / 2;
  const halfHeight = height / 2;
  const clampedRadius = Math.min(radius, halfWidth, halfHeight);
  const shape = new Shape();

  shape.moveTo(-halfWidth + clampedRadius, -halfHeight);
  shape.lineTo(halfWidth - clampedRadius, -halfHeight);
  shape.quadraticCurveTo(
    halfWidth,
    -halfHeight,
    halfWidth,
    -halfHeight + clampedRadius,
  );
  shape.lineTo(halfWidth, halfHeight - clampedRadius);
  shape.quadraticCurveTo(
    halfWidth,
    halfHeight,
    halfWidth - clampedRadius,
    halfHeight,
  );
  shape.lineTo(-halfWidth + clampedRadius, halfHeight);
  shape.quadraticCurveTo(
    -halfWidth,
    halfHeight,
    -halfWidth,
    halfHeight - clampedRadius,
  );
  shape.lineTo(-halfWidth, -halfHeight + clampedRadius);
  shape.quadraticCurveTo(
    -halfWidth,
    -halfHeight,
    -halfWidth + clampedRadius,
    -halfHeight,
  );
  shape.closePath();
  return shape;
}

function normalizeFaceUvs(
  geometry: ShapeGeometry,
  widthMm: number,
  heightMm: number,
) {
  const position = geometry.getAttribute("position");
  const uv = geometry.getAttribute("uv");
  for (let index = 0; index < position.count; index += 1) {
    uv.setXY(
      index,
      position.getX(index) / widthMm + 0.5,
      position.getY(index) / heightMm + 0.5,
    );
  }
  uv.needsUpdate = true;
}
