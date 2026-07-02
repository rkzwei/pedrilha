# Changelog

All notable changes to Gem Finder are documented here.
Versions follow `0.MINOR.PATCH` — features bump minor, fixes bump patch.

## [0.3.0](https://github.com/rkzwei/gem-finder/compare/v0.2.1...v0.3.0) (2026-07-02)


### Features

* add dynamic page titles (UX-05) ([9947ce2](https://github.com/rkzwei/gem-finder/commit/9947ce2c31c7ef8e46c43ec123769d587f19a58c))
* de-jargon Wildcards page copy (UX-28) ([89ff51c](https://github.com/rkzwei/gem-finder/commit/89ff51c11825bb072900693c353ef512e2951e54))
* explain Gem Score with /about page (UX-04) ([770e499](https://github.com/rkzwei/gem-finder/commit/770e499178d6617c2e31a13b39de6a7a4b5a36e1))
* highlight active nav link (UX-22) ([00af751](https://github.com/rkzwei/gem-finder/commit/00af7518ef2535b19d2ff227bad2212ff6142ace))


### Bug Fixes

* add branded 404 page (UX-27) ([ac45002](https://github.com/rkzwei/gem-finder/commit/ac450028db0746584314187b318c1190a49e986d))
* add loading shell to prevent blank screen on first load (UX-01) ([c754a94](https://github.com/rkzwei/gem-finder/commit/c754a94764a66d5d62d46e19e825876273e0b587))
* added hook for debugging ([4957f0f](https://github.com/rkzwei/gem-finder/commit/4957f0fbffc8d81f8168e4dcc9d43bd9db0d7d66))
* favicon absolute path for nested routes ([5ed6164](https://github.com/rkzwei/gem-finder/commit/5ed6164b8050b3e6627ec4684d9ae9e0c9504201))
* infinite loop no more ([b312258](https://github.com/rkzwei/gem-finder/commit/b312258dd5c55e05c96e22024f1977f4f9461721))
* lazy-load Ko-fi widget after app starts (UX-02) ([ea74362](https://github.com/rkzwei/gem-finder/commit/ea7436203bfb592abac1f21fa352fc9baeba51dc))
* make header nav responsive on small viewports (UX-03) ([060c1ac](https://github.com/rkzwei/gem-finder/commit/060c1acfab482cdb62af9c4bb278d6c837cfbcd7))
* prevent iOS auto-zoom on inputs by setting font-size 16px (UX-06) ([4535aee](https://github.com/rkzwei/gem-finder/commit/4535aeedf2e09e37c2cb2c9cfb6fbe8426f65dd4))
* replace Ko-fi widget script with static button ([9667264](https://github.com/rkzwei/gem-finder/commit/9667264874aa63e3a40ace8b4932a8f492529f2a))
* restore query-reactive fetching on grid pages ([1da16d6](https://github.com/rkzwei/gem-finder/commit/1da16d6f9abaafe1755ba4c4da2cd37882a1bd8c))

## [0.2.1](https://github.com/rkzwei/gem-finder/compare/v0.2.0...v0.2.1) (2026-06-29)


### Bug Fixes

* strip commit links from changelog, fix Documentary genre cutoff ([cd8c63b](https://github.com/rkzwei/gem-finder/commit/cd8c63b759765a756bd7becaef66d2397d5ef5f2))
* strip commit links from changelog, fix Documentary genre cutoff ([37e2b12](https://github.com/rkzwei/gem-finder/commit/37e2b12844ca4aab0195192f8f68dbcadea06532))

## [0.2.0](https://github.com/rkzwei/gem-finder/compare/v0.1.0...v0.2.0) (2026-06-29)


### Features

* add Ko-fi tip button ([7158e40](https://github.com/rkzwei/gem-finder/commit/7158e4003b523bffed9206378b71afaab4e301fd))
* added custom favicon ([b580369](https://github.com/rkzwei/gem-finder/commit/b580369f580b595c657ffb167cd41225e38180d4))
* admin panel, single deployment, pagination buttons, CSS rebuild, CI fix ([20f13ce](https://github.com/rkzwei/gem-finder/commit/20f13ce0d73ed9e33459f6db1cd3b5fa0b7b8d25))
* **analytics:** track_event/umami at all call sites, section_view on mount ([03a4db3](https://github.com/rkzwei/gem-finder/commit/03a4db333fd8caf777ed8362916ce3c0454b9709))
* **api:** POST /api/event anonymous engagement tracking ([229e3d6](https://github.com/rkzwei/gem-finder/commit/229e3d6d89f15db267c25f85be42a804666579ad))
* **db:** migration v6 events table + insert/purge helpers ([5b834e0](https://github.com/rkzwei/gem-finder/commit/5b834e06ffeb49897ea4a5c2d9967a47576414f3))
* **design:** Red Latitude palette, Bebas Neue, film grain, select appearance fix ([9715d21](https://github.com/rkzwei/gem-finder/commit/9715d21119e13eb595e87d8b9a8cda811b01ea89))
* dropdown filter bar — genre/era panels, click-away overlay ([27c9fd3](https://github.com/rkzwei/gem-finder/commit/27c9fd39148d68e3d96737a218f47c470c7a0bbc))
* **frontend:** analytics tracking at all call sites ([86714ee](https://github.com/rkzwei/gem-finder/commit/86714eee045ab9bd96d4dc0292848ff2a5ca9940))
* **frontend:** filter dropdown bg, search debounce 300ms, Music genre fix ([0dccff8](https://github.com/rkzwei/gem-finder/commit/0dccff888ebdd3e21048e69b93fcc8060398efb2))
* **frontend:** privacy policy page + footer link ([9c21738](https://github.com/rkzwei/gem-finder/commit/9c21738e43c01655caaf83401fd81293c1b50bcb))
* keywords/Musical filter, CORS, log rotation, rate limiting, JWT CVE fix ([b8341a8](https://github.com/rkzwei/gem-finder/commit/b8341a8fed531e43acc23b7be6e35d4f71da2e59))
* Musical filter, CORS, log rotation, rate limit, deploy files, docs ([d30836e](https://github.com/rkzwei/gem-finder/commit/d30836e8bdcacf87a9a6d5a27e93d90e0ec7db9a))
* mv+base36 routing, sc- CSS tokens, admin panel, wildcards wiring ([e2523a7](https://github.com/rkzwei/gem-finder/commit/e2523a71492d82a457ce4dce7bb1e8ac95a15776))
* Phase 8 auth + watchlist — magic link, JWT, watchlist CRUD + frontend UI ([eb49e05](https://github.com/rkzwei/gem-finder/commit/eb49e05f76a31a5436341987e9521098392447e1))
* Phase 8 auth + watchlist — magic link, JWT, watchlist CRUD + frontend UI ([99b461f](https://github.com/rkzwei/gem-finder/commit/99b461f552df24be3fa83bcbb1c33efe2c880608))
* **phase8:** sign-in page, filter overlay, SMTP status, scheduled sync ([57d5bda](https://github.com/rkzwei/gem-finder/commit/57d5bdafe398da60bee17396f7dfe0fb0115d015))
* **phase8:** sign-in page, filter overlay, SMTP status, scheduled sync ([fe77057](https://github.com/rkzwei/gem-finder/commit/fe77057ecebcc643493ec369d57e3ed90f6e1c74))
* **runner:** add self-hosted Docker runner for local machine ([a867c70](https://github.com/rkzwei/gem-finder/commit/a867c7093f1940e643c44a0fd5d4f623159c9da4))
* **scoring:** tiered RT-endorsed floor + lower sync floor to 5.5 ([aae224e](https://github.com/rkzwei/gem-finder/commit/aae224e09175559332a4eec3a8e415a386a59f71))
* sort order + watchlist status filters ([e7f29df](https://github.com/rkzwei/gem-finder/commit/e7f29df0217f1863b0cf133f7605d5c94df2172a))
* **sync:** loosen acclaimed candidate vote_count threshold 10k→5k ([629d6c3](https://github.com/rkzwei/gem-finder/commit/629d6c337a8caca31dade5d237e198301e292a68))
* versioning with release-please, changelog page at /changelog ([14c07b7](https://github.com/rkzwei/gem-finder/commit/14c07b7e4432a0272aa1f29888d646fabd5e481b))
* wildcards — RT credibility multiplier, classification, clear stale scores, remove OMDb limit ([cf9ea13](https://github.com/rkzwei/gem-finder/commit/cf9ea1390633502c4506dce832bd74ea4dd1592c))


### Bug Fixes

* **api:** genre filter AND logic (all selected genres must match) ([ce35c31](https://github.com/rkzwei/gem-finder/commit/ce35c3160c3bc4555cc18542f89758d048db1885))
* broken compile, making compile easier for lower end server ([588e896](https://github.com/rkzwei/gem-finder/commit/588e896cc137330683bd353392bf31ce6479ff4f))
* **ci:** don't rebuild when only deploy.yml changes ([b687d86](https://github.com/rkzwei/gem-finder/commit/b687d866586752b8e36c684bfb7a6a6485cc4bd8))
* **ci:** force backend+frontend when last run failed (fix no-op force steps) ([1cfa74a](https://github.com/rkzwei/gem-finder/commit/1cfa74ab2439e341fc7a5fbde7625f342400e7f1))
* **ci:** ignore deploy.yml and markdown changes in ci.yml trigger ([e400b8e](https://github.com/rkzwei/gem-finder/commit/e400b8e7770638a002eebe40d1a6b0572643b2fa))
* **ci:** restore path filtering to deploy.yml, force both on last deploy failure ([1fc81b3](https://github.com/rkzwei/gem-finder/commit/1fc81b3f9618af6559af95034effe75be5e0daed))
* clippy derivable_impls and unnecessary_map_or ([995331f](https://github.com/rkzwei/gem-finder/commit/995331fc01d79cee0a521366e7a81c56f859ae7d))
* correct include_str path for CHANGELOG.md ([bfa65c4](https://github.com/rkzwei/gem-finder/commit/bfa65c44e5673c819af92f39d56a7d7f419e6ac4))
* correct Ko-fi button container class name ([fc24bf4](https://github.com/rkzwei/gem-finder/commit/fc24bf44d39b5e339a3bdfdcc0b67877034559d0))
* **db:** exclude MCU/franchise films from acclaimed classification ([7c4bbc4](https://github.com/rkzwei/gem-finder/commit/7c4bbc4561728f6af4e98e57edc515680f3dd003))
* **deploy:** install trunk and wasm-bindgen into CARGO_HOME/bin ([a4bd443](https://github.com/rkzwei/gem-finder/commit/a4bd4437371c25ed84612271bd48f2fd4dbdbef1))
* **docker:** trunk build, CSS pipeline, Trunk.toml ([8728167](https://github.com/rkzwei/gem-finder/commit/87281671d85f4a8b28d44aa5e0363237a77104b7))
* explicit permission for read in gh ([9edf314](https://github.com/rkzwei/gem-finder/commit/9edf31418cc3be424eda9b5fe1b030a654c7a6f5))
* extract view! attribute expressions to avoid macro parse errors ([ccc4a2d](https://github.com/rkzwei/gem-finder/commit/ccc4a2d6e1198256264ea78085133d698ef7fc1b))
* incorrect example for PAT ([c1ae0be](https://github.com/rkzwei/gem-finder/commit/c1ae0bed87f3b5afd384ffcffbc7dcc8da22e583))
* jsonwebtoken ring feature — use ring directly, drop rustls dep ([a50f01e](https://github.com/rkzwei/gem-finder/commit/a50f01e1dddd257734fa3c98f7d6c38e05640532))
* jsonwebtoken rust_crypto feature — no ring, no rustls provider ([dab54b1](https://github.com/rkzwei/gem-finder/commit/dab54b1e2e4a364b6c764a3f05f6edbb8bba7a88))
* move disabled bools out of view! macro, remove early return ([4a52f14](https://github.com/rkzwei/gem-finder/commit/4a52f149b7cd02f480170292f684a4193ef61b00))
* move Ko-fi button to bottom-right ([b6004cd](https://github.com/rkzwei/gem-finder/commit/b6004cde15c0a1220523d40e88ec993c179aefed))
* pagination Next/Last buttons, page input style, search placeholder ([8dc16a2](https://github.com/rkzwei/gem-finder/commit/8dc16a2078885cfb6c49e3efae707cf5e5b70572))
* patch jsonwebtoken CVE — bump 9-&gt;10.4.0, require exp claim explicitly ([40771ac](https://github.com/rkzwei/gem-finder/commit/40771ac86739b7bcae3c3f2cad5a935e87edd679))
* permisssion issues with local runner ([a943abb](https://github.com/rkzwei/gem-finder/commit/a943abb26d6fcd05e7166be7e24efeb94b61eaf7))
* raise header z-index above filter bar, fix favicon via trunk copy-file ([ac74850](https://github.com/rkzwei/gem-finder/commit/ac748504cc8e097c7cad43fdcb539f0c67d05b6c))
* release-please simple type with toml jsonpath for workspace version ([fdb8c3d](https://github.com/rkzwei/gem-finder/commit/fdb8c3d83b237683da8647d1e805fa88faf2c364))
* **runner:** cargo volume permissions + multi-runner support ([da39707](https://github.com/rkzwei/gem-finder/commit/da39707f414c8f3881e62cd43d59805a78266a43))
* **runner:** don't share registry/src between runners ([e4a228f](https://github.com/rkzwei/gem-finder/commit/e4a228f698b9214acd6bc1b8b9020753b6add0cc))
* rustfmt ([686ebb5](https://github.com/rkzwei/gem-finder/commit/686ebb59dab8b49f1de2556fbf8b0d6e06582a57))
* scope fmt checks per job, per-job failure detection for force-rebuild ([0fe9b2f](https://github.com/rkzwei/gem-finder/commit/0fe9b2f148923fb48a5324c87ace589f65900a5a))
* **scoring:** fix encoding corruption and type errors in gem_score.rs ([1ba3b45](https://github.com/rkzwei/gem-finder/commit/1ba3b455056e89c4eb174ba766bcccc862512371))
* **scoring:** fix missing ? operators in integration test helpers ([5db278b](https://github.com/rkzwei/gem-finder/commit/5db278bdb0388be887a37cb6d84eab5642d6524f))
* **scoring:** fix smart quote string literals breaking CI build ([0c096f2](https://github.com/rkzwei/gem-finder/commit/0c096f295a490a4fbdd81b2923545a6f79f646ba))
* skipping on previous build failure ([f619fc8](https://github.com/rkzwei/gem-finder/commit/f619fc8e86707577af69b035c5aea7cc4f355b7f))
* upload binary from CARGO_TARGET_DIR, add cargo cache cleanup ([fcb38bc](https://github.com/rkzwei/gem-finder/commit/fcb38bceba08613bf07e4459bfd6a1d4df7a37fb))


### Reverts

* remove build.rs (cargo deadlock with trunk), clean CI ([26087cb](https://github.com/rkzwei/gem-finder/commit/26087cb13d0fda904799c56ad74b7bf2cc9c4e9b))

## [0.1.0] - 2026-06-29

### Features

* Hidden Gems, Acclaimed, and Wildcards pages with pagination and filtering
* Genre, era, and sort filters with URL persistence
* User accounts via magic-link sign-in (Hostinger SMTP)
* Watchlist with want-to-watch / watched states
* Movie detail pages
* Admin panel (seed, sync, enrich, score, logs)
* Analytics event tracking
* Self-hosted CI/CD with Docker runners and VPS deploy
