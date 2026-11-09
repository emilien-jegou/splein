# Riode AI Agent Guidelines

You are an expert core developer working on **Riode**, a reactive, GPU-accelerated UI framework in Rust targeting WebGPU (`wgpu`) with zero HTML/DOM and zero Skia.

## Mandatory Reading Before Coding
Whenever you need to write RSX views, custom shader stages (`Stage`, `EffectStage`), flex containers, or reactive bindings, you MUST consult the skills located in:
- `.agents/coding-style.md` (MUST READ, important coding styles instructions)
- `.agents/skills/riode-dsl.md` (Declarative DSL, Props, Components, Container Properties, Custom Stages)
- `.agents/skills/riode-architecture.md` (Workspace topography, 4-phase execution lifecycle)

## Key Architectural Invariants
1. **Zero-GPU Core Contract (`riode`)**: `riode` contains strictly 0 lines of WGSL and zero direct references to `wgpu`, textures, or render pipelines.
2. **SDF Producer vs. Consumer Duality**: 
   - `Shape` produces SDF distance (`FragmentPayload.dist`).
   - `Material` / `Overlay` consume `dist` and write/blend `FragmentPayload.color`.
   - **0 MSAA render buffers** are allocated (anti-aliasing is algebraic subpixel filtering).
3. **O(1) Retained State**: `Shape`, `Material`, `Overlay`, and `Effect` wrap immutable chains of stage boxes in `Arc<Vec<Box<dyn Stage>>>`. Cloning/diffing is $O(1)$ atomic pointer increments.
