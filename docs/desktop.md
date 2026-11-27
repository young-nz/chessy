# Desktop packaging

GitHub Pages cannot host a Holochain conductor, and a page served over HTTPS often cannot call `ws://127.0.0.1`. The desktop app is a [Kangaroo Electron](https://github.com/holochain/kangaroo-electron) bundle: one download that starts a conductor and loads the Chessy UI.

The template's `main` branch vendors Holochain **0.7.x** binaries (`bins.holochainVersion` in its `kangaroo.config.ts`). Build Chessy with hApp tooling from the same 0.7 line (`hc` 0.7.1 in this repo).

## What the release attaches

Pushing a `v*` tag runs `.github/workflows/release.yml`.

Always attached:

- `chessy.happ` — DNA and coordinator, no UI
- `chessy.webhapp` — that hApp plus the zipped UI (this is what Kangaroo's `pouch/` consumes)
- `chessy-ui.zip` — the static UI, including `icon.png`

The Linux Kangaroo job checks out `holochain/kangaroo-electron`, drops `chessy.webhapp` into `pouch/`, sets the app id to `org.chessy.desktop` and the product name to `Chessy`, then runs `yarn build:linux`. AppImage, deb, or rpm files it produces are uploaded to the same release. macOS and Windows use the template's `build:mac-arm64`, `build:mac-x64`, and `build:win` scripts on those runners; they are not in the default workflow yet.

## Build a web bundle locally

```bash
npm ci
npm run build:ui
scripts/build-happ.sh
scripts/package-webhapp.sh
```

`dnas/chessy/workdir/chessy.webhapp` is the file you put in Kangaroo's `pouch/` directory. The UI zip inside it contains `icon.png` (at least 256×256), which Kangaroo's icon script expects at the root of the UI.

## Build the Linux app locally

```bash
git clone --depth 1 https://github.com/holochain/kangaroo-electron.git /tmp/kangaroo
cp dnas/chessy/workdir/chessy.webhapp /tmp/kangaroo/pouch/chessy.webhapp
```

In `/tmp/kangaroo/kangaroo.config.ts`, set `appId` to `org.chessy.desktop` and `productName` to `Chessy`. Leave the `bins` hashes alone; they pin the conductor binaries the template downloads.

```bash
cd /tmp/kangaroo
yarn install
yarn fetch:binaries
yarn pouch:unpack
yarn create:icons
yarn build:linux
```

`yarn dev` instead of `build:linux` launches it for a local check. The window talks to the bundled conductor, so it does not depend on a browser origin or on a conductor you installed yourself.

Bootstrap and relay URLs in the template are public test servers. For a long-lived network, replace them before you ship a build to other people, and treat that choice as permanent for that DNA network.
