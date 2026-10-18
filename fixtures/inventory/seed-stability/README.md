# seed-stability

One home page and 120 pages of a single template. `options.json` uses seed `alpha`, `options-b.json` uses seed `beta`.

Checks: `expected-inventory.json` is shared (the inventory does not depend on the seed); `expected-sample.json` and `expected-sample-b.json` hold different seven-page random sets; running either options file twice gives byte-identical output.
