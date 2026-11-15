# Roadmap

What is planned for ElaraWM, in the order it is meant to happen. The README says what
works today.

## Next: making the application something people can download

`ElaraWM.app` is meant to be what people install. These are what stands between the
application as it builds today and that.

1. **A place to download it from.** The repository has no remote and no releases. Publish
   it, make a first release, and put the link to the Releases page in the README, which
   now only says "the Releases page of this repository".

2. **A release script.** One command that builds the application, signs it, zips it with
   the version in its name and says what to upload. Today `tools/bundle-app.sh` stops at
   the built application.

3. **Developer ID signing and notarization.** A downloaded ElaraWM is refused by macOS
   until the user chooses "Open Anyway" in System Settings, because it is signed with a
   self-signed certificate that means nothing on another Mac. This is the largest piece of
   friction for a new user. Fixing it needs:
   - a Developer ID Application certificate, which takes membership of the Apple Developer
     Program (paid, yearly). **This is a decision to take first.**
   - signing with the hardened runtime, submitting each release to Apple's notary service
     and stapling the result, as steps of the release script;
   - taking the "Open Anyway" step and the `xattr` fallback out of the README.

   If the decision is no, this item is dropped and the README stays as it is.

4. **Permissions that survive an update.** macOS ties the Accessibility and Screen
   Recording grants to the application's signature. Releases signed with one and the same
   certificate should keep them from version to version, but this has not been tried on a
   second Mac. Check it with two releases in a row, and write down in the README what was
   seen. Every release must be signed with the same identity; an ad hoc signed one would
   make every user grant the permissions again.

5. **Which Macs and which macOS.** The application is built for Apple silicon only, and
   has only been run on macOS 27, though its bundle declares macOS 14 as the minimum.
   Decide the oldest macOS to support and try it there. A build for Intel Macs only
   matters if that is a version Intel Macs can run; if so, build a universal binary in the
   release script. Then make the bundle's minimum version and the README's requirements
   say the same thing.

## Later

- **`elara msg` and the IPC socket**: niri's way of scripting the window manager and
  listening to its events. `niri-ipc` is already imported.
- **More than one display**: the code is there (the monitor actions, workspaces per
  display, plugging and unplugging) and has had almost no real use. It needs a pass on a
  real setup.

## Not planned

- **Screenshots.** macOS has its own.
- **Window shadows.** macOS gives windows their own.
- **A focus ring or border around a window's picture** while its workspace slides past the
  menu bar. It is fine as it is.
