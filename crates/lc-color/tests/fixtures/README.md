# Camera colour reference vectors

Regenerate using `scripts/engine_colour_refvec.py ENGINE_SOURCES` from the repository root;
see `crates/lc-pipeline/tests/fixtures/README.md` for harness build/run details.
RawTherapee source snapshot: `5f486d3678b34c74ba0c63571c17babe20935019`.

- `dcp.csv`: 48 deterministic `rtengine/dcp.cc::hsdApply` evaluations covering wrapped hue,
  saturation interpolation, and both 2.5D and 3D tables (linear value encoding).
  The extracted evaluator retains upstream arithmetic. Standalone type declarations replace
  the application headers; unused sRGB global-LUT branches are stubbed (no generated vector
  calls them). Compare hue/saturation/value, absolute tolerance **4e-5** (hue in degrees).
  Rust's analytic sRGB transfer is separately tested by the profile unit tests.
- `illuminants.csv`: seven temperatures including both endpoints and extrapolation bounds,
  evaluated by upstream `mix3x3` and the inverse-CCT interpolation formula from
  `findXyztoCamera`. The harness supplies CCT directly, isolating interpolation from each
  application's CCT estimator. Rust xy↔CCT round trips and matrices agree within **1e-6**.
  ForwardMatrix, AnalogBalance, CameraCalibration and re-derived WB have separate Rust tests.

No DNG SDK code or proprietary curve/profile data is bundled. See
`licenses/rawtherapee-NOTICE.md` for attribution and intentional headroom differences.
