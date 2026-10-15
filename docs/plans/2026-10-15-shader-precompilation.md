# Optional shader pre-compilation (plan, 2026-10-15)

Asked in #184 ("chunks of the game missing + frame rate dropping"). Goal: a launcher option that
builds shaders and pipelines ahead of time, so the first visit to an area does not stutter or show
objects late. It is off by default, because upstream moved away from bulk preloading (d31e360).

## What can be pre-compiled

The shaders are GCN code inside the game. A pipeline also depends on draw state (vertex format,
blending, render targets) that is only known when the game draws. So "ahead of time" can only mean
"from a record of what the game drew before". There are two sources:

1. **The player's own store** (`user/cache/CUSA01623`): every program, permutation and pipeline
   key from earlier sessions. Since `DiskShaders`, its permutation indices are stable, so a bulk load
   is consistent.
2. **A shared store** (later phase): metas, SPIR-V and pipeline keys are portable across machines.
   The driver binaries are not. Limits:
   - `CheckCacheProfile` clears a store whose `Shader::Profile` differs, so a shared store would
     be per GPU vendor or profile.
   - The SPIR-V is translated from the game's code, so redistributing it may be a copyright
     problem. Decide before building it.

## Launcher option

Performance > Shaders: **"Shader pre-compilation"**, a choice passed as
`GOW3_SHADER_PRECOMPILE`:

| Value | Behavior |
|---|---|
| `off` (default) | Current behavior: on-demand startup with disk shaders and prefetch. |
| `background` | Start at once. Low-priority threads build every saved pipeline while you play; the corner note shows "Pre-compiling shaders N%". |
| `start` | Build everything before the first frame, on the existing loading screen (`Gow3Overlay::SetLoading`). Longest start, no stutter for anything already seen. |

Hint text: "Builds the shaders saved from earlier sessions ahead of time. Background: the game
starts at once and stutters less as it goes. Before start: a longer loading screen, then no shader
stutter in areas you have visited. Switch it off if the frame rate is lower after it finishes or
the game uses too much memory."

## Implementation

The pieces already exist: `WarmUp` (loading screen, parallel `WarmupPool`) and
`StartBackgroundWarmUp` plus `PumpWarmUp` (a few results per frame through `WarmupInbox`). They are
bypassed whenever the pipeline binary cache is supported (`UseOnDemandPipelines`).

1. **Measure the regression first.** d31e360 says bulk preloading lowered FPS after it finished,
   with no recorded cause.
   - Add `GOW3_SHADER_PRECOMPILE` to `vk_presenter.cpp`: `start` calls `WarmUp`, `background`
     calls `StartBackgroundWarmUp`, ahead of `UseOnDemandPipelines`.
   - Compare FPS and p99 in one scene against `off`, with GOW3_PERF_DIAG and a 5 s window after the
     warm-up ends.
   - Suspects to check:
     - the warm-up threads still running;
     - the size of `graphics_pipelines` (~4k entries) and driver memory per pipeline;
     - the prefetch and disk-shader paths duplicating programs;
     - the `TryPreparedPipeline` hit rate changing.
2. **Fix what the measurement shows**, for example:
   - stop the pool threads explicitly when the warm-up ends;
   - build pipelines for the current area first (key order by last use, if recorded).
3. **Background mode on top of on-demand:**
   - Keep `StartOnDemand` (disk shaders, prefetch, binary conversion).
   - Add a low-priority builder that walks the `g_*.key` blobs with `LoadGraphicsPipeline` into the
     inbox, so the GPU thread only moves finished pipelines in (`PumpWarmUp`).
   - Skip keys already in `graphics_pipelines`.
   - Report progress through a new corner note (`Gow3Overlay::SetShaderPrecompile`), stacked like the
     prefetch note and hidden by the "Show shader notes" option.
4. **Mode selection as a pure function** next to `UseOnDemandPipelines` in `vk_warmup_inbox.h`,
   with cases in `tests/test_warmup_inbox.cpp`.
5. **Launcher:** `shader_precompile` in `APP_DEFAULTS` (`'off'`), a `choice` row with the three
   values, the env mapping in `game_environment`, and pt/es/ru texts.
   `tests/test_launcher_controls.py` checks the env for each value.
6. **Shared store (later):** an import button that copies a store into `user/cache/CUSA01623`
   when its `profile.bin` matches. A mismatch is refused with a message, never cleared.

## Risks

- **Longer start (`start`) or CPU load in the first minutes (`background`):** low priority, and at
  most the warm-up thread count (`WarmupThreadCount`).
- **Memory:** every saved pipeline stays resident. Report the count and driver time at the end.
- **The upstream FPS loss:** the option stays off by default until step 1 explains it.
- **First visits stay uncovered:** pre-compilation only covers what the store already holds.

## Done when

- Two runs of one save show "compiled" near 0 on the "Disk shaders" line from the first frames, with
  `background` and `start`.
- FPS after the warm-up matches `off` in the same scene.
- The launcher option and the overlay note work, with tests for the mode choice and the launcher
  env.

---

# Revision 2 (2026-10-15): pre-compile from a shipped seed

The owner chose "option 2": the game must pre-compile what it will need, not only what this
player already saw. `off` stays lazy (#386). `background` and `start` build from a **seed**.

## Spike result (done)

Scanning the game dump (`God of War III Remastered`, 48 GB) for the `OrbShdr` header
(`amdgpu/regs_shader.h`):
- The GCN shaders are stored in clear in `EXEC/WAD/PS4_LE/*.WAD` (345 of 498 scanned files,
  136,241 headers) and `eboot.bin` (23).
- 97,567 unique `shader_hash` values (types 0: 48,781, 1: 48,780, 4: 6). Most are material
  permutations a given run never draws; a full run uses roughly 600.
- The player's store uses the same hash: 162 of its 165 programs are found in the WADs (98 %).
  So a store key can be resolved to game code without playing.
- Not yet known: what the translator needs besides the code (see Spike 2).
- Scan output: `out/orbshdr-unique.json` (hash → type, length, count, first WAD).

## What the seed is

A file we ship (no game code, no SPIR-V): for each pipeline the game was seen to build,
- the pipeline key (the existing `g_*.key` blob: state, formats, blending, stage hashes);
- per stage: shader hash, permutation spec and the recorded translation inputs (user data,
  fetch shader), and where the code lives (WAD name, offset, length, crc32 from the header).
It is produced by a tool from a complete store, so the more of the game it was recorded on, the
better the coverage. Stage code is read from **the player's own game files** at run time.

## Phases

1. **Spike 2: headless translation.** From a store's `.meta` plus the code found in the WAD,
   re-create the SPIR-V and compare it with the stored `.spv` for the 162 matching programs.
   Needs: the code start (header position minus `(code[1]+1)*8` bytes when the shader starts with
   `0xBEEB03FF`, otherwise a backward search), and whatever `Shader::Info`/`RuntimeInfo`/profile the
   translator reads. Go/no-go for the whole plan: if the SPIR-V is not reproducible from the seed
   data, the seed would have to carry more state, or the idea fails.
2. **Seed format and tool.** `PipelineSeed` read/write in `gpu/` (versioned; refuses a different
   `PipelineKeyVersion` or translator version), and `tools/` exporter that turns a store into a
   seed. Test: round trip.
3. **Importer.** For each seed pipeline missing from the store: read stage code from the WAD
   (validate hash and crc32; fall back to a hash index of the WADs built once and cached), translate,
   write meta+SPIR-V and the key into the store (stable permutation indices, as in #386). After that
   the pipelines exist in the store and the existing `WarmUp` / `StartBackgroundWarmUp` build them.
4. **Modes** (mode function in `vk_warmup_inbox.h`, already added):
   - `background`: the importer and the builder run on low-priority threads while the game runs.
     Progress shows as a bar at the bottom of the screen, like the cache pre-load.
   - `start`: the same work before the first frame. The launcher shows a dedicated Tk window with
     a progress bar, fed by progress lines on the game's stdout (`GOW3_PRECOMPILE done total`).
     The game window stays hidden until the work ends.
5. **Launcher:** `shader_precompile` row (off/background/start), env mapping, Tk progress window,
   texts in pt/es/ru, tests.
6. **Measure** (the original step 1): FPS and p99 after the warm-up against `off`, memory, time of
   the first `start`, and why upstream's bulk preload lowered FPS (d31e360).

## Risks and open questions

- Translation time of thousands of shaders at start (CPU), plus driver compile of every pipeline
  (the long first `start`). Later starts reuse the store and the driver binaries.
- Seed coverage depends on who records it. Needs a full-game store; a list of contributors' stores
  can be merged.
- Game dump versions differ (EU/US, update): WAD offsets can differ, hashes should not, so keep the
  hash index fallback.
- Per-GPU profile: `CheckCacheProfile` clears the store when `Shader::Profile` differs; the seed is
  profile independent but the translated store is not.
- The 3 programs of 165 not found in the WADs (probably runtime-generated): they stay lazy.

## Advisor review (Opus 5.5) and what it changed

1. **Keys carry permutation numbers.** Confirmed in `RefreshGraphicsStages`:
   `key.stage_hashes[i] = HashCombine(params.hash, perm_idx)` (`vk_pipeline_cache.cpp:1241`, `:1288`).
   A seed recorded on another machine therefore uses that machine's numbering. The importer
   cannot copy keys: for each stage it must find the player's index for the same spec (or take a
   new one with `NewPermutationIndex`) and **rewrite the key's stage hashes** before saving it,
   or the game's draw-time key never matches. In `background` it must allocate under
   `programs_mutex` and respect `Program::pending`, like `MergeWarmStage`. The seed therefore
   stores `(program hash, spec)` per stage, not the combined hash.
2. **Spike 2 first, as a startup debug mode** (no new tool): for each stored meta, find the code in
   the WAD by hash, run the same `TranslateProgram` + `EmitSPIRV` as `CompileModule`, and compare
   with the stored `.spv` byte for byte. Report matched / different / missing for the 162 found
   programs. Inputs available in the meta: `StageSpecialization` holds `runtime_info` and the
   bindings `start`, and `Info` is serialized, so the seed may need nothing more.
3. **Who records the seed** decides the coverage; asked of the owner.
4. **Order:** Spike 2 before the Tk window and the launcher row (it decides whether progress has
   one phase or two: import, then build). For `start` the SDL window already exists when the
   presenter is built, so it must be hidden explicitly.

## Spike 2 result (2026-10-15): GO

`GOW3_SPIKE_TRANSLATE=<index>` (debug mode in `PipelineCache::SpikeTranslate`) re-translated every
saved shader from the game's WAD code and compared the SPIR-V with the saved one:
**381 of 381 identical** (6 programs have no code in the WADs and stay lazy). Needed beyond the code:
- the recorded `RuntimeInfo` and bindings (already in the meta's `StageSpecialization`);
- the recorded flattened resource data (`Info::flattened_ud_buf`), replayed through
  `Info::recorded_flat_buf` instead of walking the guest's resource tables (fixed the SRT shaders);
- for vertex shaders, the attribute V#s, now saved as an optional tail of the meta
  (`FetchBuffersMagic`); stores written before this build lack them.
The WAD code of a shader starts at the `0xBEEB03FF` token that precedes its `OrbShdr` header and is
`BinaryInfo.length` bytes long. Not covered: geometry shaders (their runtime info points at guest
memory); none were in the test store.

So the seed needs no more than the store already holds (pipeline keys, metas) plus the WAD
location of each program. The SPIR-V is rebuilt on the player's machine.

## Seed import (built, not yet verified in the game)

- `tools/export_shader_seed.py` packs a recorded store: `meta/*.meta`, `key/*.key`, and
  `locations.txt` (hash, file, code start, length) found by the OrbShdr header. 3.6 MB for the
  first partial recording (2044 metas, 1078 keys).
- `PipelineCache::ImportSeed` (runs inside `StartOnDemand`, before the prefetch thread):
  translates each seed shader again from the game's files (`TranslateSaved`), saves it at the
  first free permutation number, skips a permutation whose spec is already saved, renumbers the
  stage hashes of every key (`std::hash<GraphicsPipelineKey>` names the file), then waits for
  the store's writer (`DataBase::WaitIdle`) and indexes again. A `seed` marker (size, mtime, game
  folder) makes later starts skip the import.
- `start` passes `GOW3_SHADER_SEED` and `GOW3_GAME_DIR`; progress goes to the loading screen and to
  stdout as `GOW3_PROGRESS import|build <done> <total>` lines for the launcher's window.
- Tests: `ParseSeedLocation`, `FreePermutationIndex` (test_warmup_inbox), the exporter
  (test_export_shader_seed.py). The importer itself needs the game: two runs, below.

Verification runs (advisor): (A) empty store, `start` with the seed: cold import time and counts.
(B) store with its own first-use numbering (play the title and one area in `off`), then `start`
with the seed: "already saved" > 0, no "conflicts with index" warnings, `compiled` near 0.

Open: `background` needs the import too (decide from the measured cold time: loading screen once,
or a worker thread with its own pools); a run on an AMD/Intel GPU (the spec is built with the
profile); scan the `Update` folder for the 5 programs missing from the WADs.

## Findings and where this stands (2026-10-15)

**Can every shader the game needs be pre-compiled from the game files alone? No.**
The files hold only the GCN code (97,567 unique shaders in `EXEC/WAD/PS4_LE/*.WAD` and `eboot.bin`,
found by the `OrbShdr` header). Everything else a pipeline needs is made by the game's code while it
runs: resource descriptors (V#/T#, resource tables), vertex attribute descriptors, run-time info,
and the pipeline state (blending, render-target formats, sample counts, read from GPU registers on
each draw). Spike 2 showed it: the 27 resource-table shaders and the 86 vertex shaders only
re-translated byte-identical after their recorded data was replayed. So the only portable source is
a **recording** of what the game drew.

**What works (measured on the RTX 5070 test machine):**
- A recording (a store) is exported to a seed with `tools/export_shader_seed.py`: pipeline keys,
  shader metas and the WAD location of each shader's code (3.6 MB for 2044 metas / 1078 keys, no
  SPIR-V, no game code, no driver binaries).
- `start` and `background` import the seed (`PipelineCache::ImportSeed`): every shader is
  translated again on the player's machine from their own WAD files with their own GPU profile
  (2038 of 2044 in 3.6 s), renumbered to the player's permutation numbers, and its pipelines are
  built by the existing warm-up (1063 of 1066 in 8.4 s on 6 threads; 12 s for a first start,
  1.9 s for the next). With a store that already numbered shaders its own way (test B): 473
  "already saved", 828 pipelines added, `Disk shaders: compiled` near 0.
- Hardware independence: the seed holds no GPU-specific data. 0 of 1075 recorded keys carry
  `vertex_buffer_formats`, the one key field that depends on the GPU (only set when the driver lacks
  vertex-input dynamic state). On such a GPU those pipelines compile lazily as before. Not tested on
  another GPU (only the RTX 5070 was available): a run on AMD/Intel should show "compiled" near 0.
- Record with default graphics options (no `no_msaa`, no experimental upscaler): they change sample
  counts and add an output, so those pipelines would not match.

**Open (for the repo owner):**
1. Coverage is what has been recorded. The test seed comes from an early-game recording; a full
   playthrough, or a merge of players' stores, is needed. The importer would take a folder of seeds
   (one per store; each is consistent and the renumbering handles the rest): small change, not done.
2. Shipping `shader_seed.zip` (`packaging/windows/shader_seed.zip`, copied by `package.sh` when
   present, not committed): it holds saved resource data and no shader code, but `package.sh`
   refuses `*.spv` and cache folders, so the owner should decide.
3. 3 pipelines still log `Cached permutation 0 ... conflicts with index 1` (two permutations whose
   specs compare equal in one direction only); they stay lazy.
4. The 6 programs with no code in the WADs (maybe in the Update folder) stay lazy.
5. Not measured: FPS after the warm-up against `off`, and why d31e360 found a lower FPS after a bulk
   preload (plan step 1). The option stays off by default until that is measured.

**Launcher:** Performance > Shaders > "Shader pre-compilation" (off / while playing / before the game
starts), `GOW3_SHADER_PRECOMPILE`; the seed is taken from `shader_seed.zip` beside the launcher
(`GOW3_SHADER_SEED`). `start` shows the in-game loading screen with the titles "Preparing shaders
from your game files..." and "Compiling shaders...". A separate Tk progress window was built and
dropped: the in-game screen looked better.
