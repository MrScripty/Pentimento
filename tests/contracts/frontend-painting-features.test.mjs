import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';

const manifest = fs.readFileSync(new URL('../../crates/app/Cargo.toml', import.meta.url), 'utf8');
const featureSection = manifest.match(/^\[features\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m)?.[1];

// The scene crate's transitive painting dependency does not put `painting`
// into the app's extern prelude. Each frontend compiling the shared dispatcher
// must activate the app's optional dependency itself.
for (const frontend of ['cef', 'dioxus', 'egui']) {
    test(`${frontend} activates painting for the shared app UI command dispatcher`, () => {
        assert.ok(featureSection, 'app Cargo.toml must declare a features section');
        const declaration = featureSection.match(new RegExp(`^${frontend}\\s*=\\s*\\[([^\\]]*)\\]`, 'm'));
        assert.ok(declaration, `${frontend} feature must exist`);
        assert.match(declaration[1], /"dep:painting"/, 'activate the direct app dependency, not only the scene feature');
    });
}
