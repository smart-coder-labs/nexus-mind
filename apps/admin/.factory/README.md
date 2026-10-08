# Factory files for the admin app

`ui-fixtures.json` lets the factory's UI specialist render this app offline: it answers the app's API calls with fake data, maps source files to the routes that show them, and names the themes to capture. The specialist builds the app in a sandbox, serves it, and screenshots the routes a change touches at a phone and a desktop viewport for its review.

The format, matching rules and defaults are in [docs/factory/ui-fixtures.md](../../../docs/factory/ui-fixtures.md). When you add a page, add the endpoints it calls on load and a `route_hints` entry for its source file.

Agents never change these files: the specialist's path check rejects any change under `.factory/`.
