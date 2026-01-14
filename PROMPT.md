# [Feature Name]

Create a website for Ralph Workflow. This is an open source project licensed with AGPL.

## Goal
You must use the claude skill frontend-design to design this website. Ensure this site has commercial level quality despite being an open source project.

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

## Feel Free
* Make RADICAL changes if needed
* Do not feel constrained by existing code, everything is experimental, if you think we need to use tailwind, use tailwind, if you think we need to get rid of an entire section, get rid of entire section. Acceptance criteria is the must important. Messy code is better deleted than kept around.
* However once you are settled on a design you think has potential, start iterating on it

## Acceptance Checks
* Working professional website that is at the quality of dribble showcase.
* No design flaws. Bad usability in general.
* User should be able to navigate through the website and have a clear understanding on how to use Ralph Workflow. They should not come out with even more questions about what we do.
* Remember index.html is a must for this, this will be used as the home page.
* From the visual output, not from code as your reference point for visuals. They **MUST** meet all **frontend-design** skill criteria

## Constraints
* This is used on a static page on codeberg. You must look up all the restrictions codeberg pages has and respect them. If you are adding a feature, ensure it works with them.
* You MUST use **frontend-design** skill from claude skill. If you cannot find it, you must look it up in ~/.claude/skills
* You MUST use playwright to visually evaluate the visual output, not from code as your reference point for visuals. They **MUST** meet all **frontend-design** skill criteria
