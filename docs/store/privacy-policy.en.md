# Kryptos Privacy Policy

> Draft to review before publishing. This is not legal advice.

**Last updated:** [DATE]  
**Controller:** Simone Vella, [ADDRESS, IF REQUIRED]  
**Contact:** [CONTACT EMAIL]

## In short

Kryptos works offline. We have no servers, there is no account, and we receive none of your data: no passwords, no statistics, no information about how you use the app.

## Data that stays on your device

Passwords, 2FA codes, `.env` files, notes and settings live in an encrypted vault on your device. The key depends on your master password, which is not stored. If you forget it, we can't recover it.

## No collection

Kryptos contains no telemetry, analytics, advertising or tracking. Site logos are bundled in the app: no network request reveals which sites you saved.

## Browser extension

The extension talks only to the Kryptos app on your computer, through the browser's Native Messaging. To find logins, it sends the current page's address to the local app. On pages, a small script notices a click on a login field: it doesn't read what you type, and you can turn it off from the popup. When you read a 2FA QR code, the extension captures the visible tab and decodes it locally, without storing it. None of this reaches us.

## Android app

The release version has no internet permission. The camera is used only to read QR codes, and images stay on the device. The autofill service sees apps' login fields to offer you logins and sends nothing.

## Services you choose

These features are optional and involve third parties, each with its own policy:

- **Cloud sync.** If you choose a Google Drive, iCloud Drive, Dropbox or OneDrive folder, Kryptos writes an encrypted copy of the vault there and your cloud app moves the file. The provider holds encrypted data, plus the file's metadata.
- **Teams.** Team data passes through the shared cloud folder, encrypted, and is visible to members who hold the key.
- **AI assistants.** If you turn on access and confirm a request, that password or code is handed to the assistant and so to its provider.

## Your rights

We process no personal data, so we hold nothing to show, correct or delete. You can erase everything from the app or by uninstalling it. For questions, use the contact above.

## Children

Kryptos isn't aimed specifically at children and collects no one's data.

## Changes

Changes to this policy will be published at this address, with the new date.
