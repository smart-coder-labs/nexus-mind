# Delta for SDD Artifact Admin

## ADDED Requirements

### Requirement: SDD Navigation and Route Are Gated by sdd:read

The system MUST render the SDD navigation entry only for users holding `sdd:read`, and MUST route a direct navigation to `/sdd` by an unauthorized user to the admin app's standard unauthorized handling rather than to an empty or erroring page.

#### Scenario: Nav item visible with sdd:read

- GIVEN the current admin user holds `sdd:read`
- WHEN they view the sidebar navigation
- THEN an "SDD" nav item is visible in the Knowledge group

#### Scenario: Nav item hidden without sdd:read

- GIVEN the current admin user holds no `sdd:read` grant
- WHEN they view the sidebar navigation
- THEN the "SDD" nav item is not rendered

#### Scenario: Direct navigation without permission is denied

- GIVEN the current admin user lacks `sdd:read`
- WHEN they navigate directly to `/sdd`
- THEN they are redirected to the app's standard unauthorized/401 handling
- AND no SDD data is fetched

### Requirement: The SDD Page Lists Changes With a Phase Pipeline Driven by Real Artifacts

The system MUST render a list of every change visible to the user across all projects, showing name, title, project, status, and a phase pipeline (`propose → spec → design → tasks → apply → verify`). The pipeline's completed steps MUST be derived from which artifacts actually exist for the change, not solely from the change's advisory `phase` field. The page MUST support filtering by project, phase, and status, and MUST show a loading skeleton while fetching and an `EmptyState` when no change matches the filters.

#### Scenario: The pipeline reflects the artifact inventory, not a stale phase

- GIVEN a change whose `phase` field is `spec` but which has both a `design` and a `tasks` artifact
- WHEN the SDD list renders that change
- THEN the pipeline shows the design and tasks steps as present
- AND the display is not limited to the `spec` step

#### Scenario: Filtering by phase updates the list

- GIVEN changes exist in several phases
- WHEN the user selects a phase filter
- THEN only changes matching that phase are displayed
- AND the underlying query is refetched with the new filter

#### Scenario: Empty state when no change matches

- GIVEN the API returns zero changes for the current filter selection
- WHEN the page renders
- THEN an `EmptyState` is shown instead of an empty table

#### Scenario: Loading state precedes data

- GIVEN the changes query is in flight
- WHEN the page renders
- THEN a skeleton is shown
- AND it is replaced by the table or the empty state once the query settles

### Requirement: A Single Shared Markdown Primitive Renders GFM Across the Admin

The system MUST provide one shared `<Markdown>` component used by every markdown rendering site in the admin, and MUST configure it with GitHub-Flavored Markdown. GFM task lists (`- [ ]` / `- [x]`) and GFM tables MUST render as a checkbox list and as a table respectively — rendering them as literal text is a defect, because `tasks.md` consists almost entirely of checklists. The four pre-existing markdown call sites (Memories, Conventions, the org memory graph, and the memory graph tab) MUST be repointed at this primitive rather than each keeping a copy of the component override map. The primitive MUST NOT execute raw HTML or script embedded in artifact content.

#### Scenario: A GFM task list renders as checkboxes

- GIVEN artifact content contains the lines `- [ ] Write the migration` and `- [x] Write the spec`
- WHEN it is rendered through `<Markdown>`
- THEN the output is a task list with one unchecked and one checked item
- AND the literal characters `- [ ]` are not shown as text

#### Scenario: A GFM table renders as a table

- GIVEN artifact content contains a pipe-delimited GFM table
- WHEN it is rendered through `<Markdown>`
- THEN the output is an HTML table with header and body rows
- AND the pipe characters are not shown as text

#### Scenario: The existing call sites use the same primitive

- GIVEN the Memories, Conventions, org memory graph, and memory graph tab views each rendered markdown before this change
- WHEN they render markdown after this change
- THEN each renders through the shared `<Markdown>` primitive
- AND no view retains its own duplicated component override map

#### Scenario: Embedded HTML in artifact content is not executed

- GIVEN artifact content contains a `<script>` tag or an inline event-handler attribute
- WHEN it is rendered through `<Markdown>`
- THEN the script is not executed
- AND the content is rendered as inert markdown output

### Requirement: The Change Detail Drawer Shows Artifact Tabs With Revisions and a Raw Toggle

The system MUST open a change's detail in a right-side drawer presenting one tab per artifact kind that exists on the change (Proposal, Specs, Design, Tasks, Verify), rendering the selected artifact's content through `<Markdown>`. The drawer MUST offer a Raw/Preview toggle that shows the artifact's markdown source unrendered, MUST offer a revision selector that refetches and displays a specific revision's content, and MUST list the change's linked tasks and linked memories. Tabs for artifact kinds the change does not have MUST NOT be rendered.

#### Scenario: Only existing artifact kinds get tabs

- GIVEN a change has a `proposal` and a `design` artifact but no `tasks` artifact
- WHEN the user opens the change detail drawer
- THEN Proposal and Design tabs are rendered
- AND no Tasks tab is rendered

#### Scenario: The Specs tab lists one entry per capability

- GIVEN a change has three `spec` artifacts with distinct capabilities
- WHEN the user opens the Specs tab
- THEN all three capabilities are selectable
- AND selecting one renders that capability's spec content

#### Scenario: Selecting an older revision refetches and renders it

- GIVEN the displayed artifact is at revision 3
- WHEN the user selects revision 1 from the revision selector
- THEN the admin fetches that specific revision
- AND renders revision 1's content in place of revision 3's

#### Scenario: The raw toggle shows unrendered source

- GIVEN an artifact is displayed in preview mode
- WHEN the user switches to Raw
- THEN the artifact's markdown source is displayed verbatim, unrendered
- AND switching back restores the rendered preview

#### Scenario: Linked tasks and memories are shown

- GIVEN a change has two linked tasks and one linked memory
- WHEN the user opens the change detail drawer
- THEN both tasks are listed with their status
- AND the linked memory is listed

### Requirement: The Admin Is Read-Only Over Artifacts

The system MUST NOT expose any way to create, edit, or delete artifact content from the admin. Artifacts are written by the harness and by git; the admin reads them and manages links. No admin view may issue an artifact-save request.

#### Scenario: No artifact editing control exists

- GIVEN a user with `sdd:write` opens any artifact tab in the change detail drawer
- WHEN the drawer renders
- THEN no edit, save, or delete-artifact control is presented for the artifact content

#### Scenario: The admin issues no artifact writes

- GIVEN a user navigates the entire `/sdd` section and opens artifacts and revisions
- WHEN the network activity is observed
- THEN the admin issues only SDD read requests for artifact content
- AND MUST NOT issue an artifact-save request

### Requirement: The Task Detail Cross-Links to the SDD Change

The system MUST render each of a task's linked spec change names as a navigable link into the SDD section filtered to that change, and MUST show the change's phase alongside the name when the change is known to the SDD store. A linked name with no matching change (a dangling link, e.g. after a rename) MUST still be displayed, without a broken navigation target and without failing the task detail render.

#### Scenario: A linked spec name navigates to its change

- GIVEN a task is linked to the change name `"sdd-artifacts"` and that change exists
- WHEN the user opens the task detail and clicks the linked spec name
- THEN they are navigated to the SDD section scoped to that change

#### Scenario: The change's phase is shown next to the link

- GIVEN a task is linked to a change currently in phase `design`
- WHEN the user opens the task detail
- THEN the linked spec entry displays the change's phase

#### Scenario: A dangling spec link renders without breaking the view

- GIVEN a task is linked to a change name that has no matching SDD change
- WHEN the user opens the task detail
- THEN the name is still displayed
- AND it is not rendered as a link to a non-existent change
- AND the task detail renders without error

### Requirement: SDD Results Appear in the Admin Global Search

The system MUST render an SDD result group in the admin's global search, listing matching changes and navigating to the change on selection. The group MUST be omitted rather than rendered empty when the search returns no SDD results, and its absence MUST NOT break the rendering of the other result groups.

#### Scenario: SDD results are grouped and navigable

- GIVEN the global search API returns two matching changes in its SDD facet
- WHEN the user runs that search in the admin
- THEN an SDD group lists both changes with their name and phase
- AND selecting one navigates to that change in the SDD section

#### Scenario: No SDD results means no SDD group

- GIVEN a global search returns memory results but an empty SDD facet
- WHEN the results render
- THEN no SDD group is displayed
- AND the memory results render normally

#### Scenario: A user without sdd:read sees no SDD group

- GIVEN the current admin user lacks `sdd:read`
- WHEN they run a global search that would otherwise match a change
- THEN no SDD group is shown
- AND the remaining result groups render normally
