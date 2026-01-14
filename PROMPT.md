# [Feature Name]

Create a website for Ralph Workflow. This is an open source project licensed with AGPL.

## Goal
You must use the claude skill frontend-design to design this website for Ralph Workflow. Ensure this site has commercial level quality despite being an open source project.
The overarching idea behind Ralph Workflow is Ralph which is an idea from Geoffrey Huntley's article here: https://ghuntley.com/ralph/

This is essentially a more organized way of looping the AI agent. The whole point of this is to let the AI cook things overnight or walk away from the computer for 8 hours and come back. 
However the user must provide a very detailed product specification for the AI to work with in order to do well.

## Questions to Consider
Before implementing, think through:

**Clarity:**
* What pages do I need in this website?
* What sections do I need on various pages to ensure that the user knows about our open source project, what it can do, and how they can use it in their workflow.
* Is our site clear for software developers? what about vibe coders? what about people new to the command line?
* Are we just rehashing everything in README.md? Or are we actually helpful as a website?
* How is logo handled? what about favicon? We don't have them in this project, what should we use instead?

Installation Instructions:
* If we have installation instructions, ensure they only have git clone from our git repository at ssh://git@codeberg.org/mistlight/Ralph-Workflow.git our cargo name is ralph-workflow

## Resources
* Original Article: https://ghuntley.com/ralph/
* Codeberg repository: https://codeberg.org/mistlight/Ralph-Workflow

## Feel Free
* Make RADICAL changes if needed
* Do not feel constrained by existing code, everything is experimental, if you think we need to use tailwind, use tailwind, if you think we need to get rid of an entire section, get rid of entire section. Acceptance criteria is the must important. Messy code is better deleted than kept around.
* However once you are settled on a design you think has potential, start iterating on it

## Acceptance Checks
* The visual quality must meet professional, commercial-grade standards (Dribbble-level polish).
* Design decisions must be evaluated from rendered output, not from code structure.
* User should be able to navigate through the website and have a clear understanding on how to use Ralph Workflow. They should not come out with even more questions about what we do.
* The site must be:
  - Clear to software developers
  - Approachable for newcomers to the command line
  - Intuitive for “vibe coders”
* Accessibility basics are required:
  - Readable contrast
  - Keyboard navigability
  - Clear focus states
  - Responsive design across common viewport sizes

## Constraints
* The website must be fully static and compatible with Codeberg Pages.
* Only pre-built static assets may be committed (HTML, CSS, JS, images, fonts).
* No server-side code, edge functions, or runtime build steps are allowed on Codeberg Pages.
* Any build tools (e.g. Tailwind, bundlers, minifiers) may be used locally only; the repository must contain the final compiled output.
* The site must function correctly when opened directly via index.html without a development server.
