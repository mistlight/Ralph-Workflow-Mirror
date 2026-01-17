## Design Stabilization (Frontend-Design Required)

### Mandatory tool usage

You **must use the `claude` skill `frontend-design`** for this task. 

Two items in this task, first item most important!!
* Ensure this works on Codeberg Pages, meaning you MUST look up the codeberg documentation and ensure this repository WILL RENDER all vite generated artifacts CORRECTLY
* This is also fundamentally about **visual system correction, layout coherence, and design consistency**, not code correctness alone. All decisions must be validated against **rendered visual output**, not abstract reasoning.

Failure to use `frontend-design` for diagnosis and implementation is considered a failure of the task.

## Goal

Make improvements by focusing on small details until it's up to par with `frontend-design` skill. You should only focus on little details and not about the overall design language at this point.
* When uploaded to codeberg pages it MUST work, the index.html in the main directory (not dist) MUST render without going through vite dev or opening a server, it should work by opening it from the filesystem even if the artifact is built with vite (the current build doesnt work, just look at https://mistlight.codeberg.page/Ralph-Workflow/docs/workflows.html lol)
* YOU MUST USE ALL THE POSTCSS RULES AND REFACTOR
* MOVE JAVASCRIPT TO TYPESCRIPT WITH STRICT MODE
* VITE OUTPUT MUST BE COMITTED, we are an open source project so no point really hiding our actual source since they are open source anyway but it's for user experience to minify and load the site faster

## CONSTRAINT
* YOU ARE NOT HERE TO COME UP WITH A COMPLETELY NEW DESIGN YOU MUST ITERATE ON THIS EXISTING DESIGN
* You MUST look up Codeberg documentation when you analyze if this will work on Codeberg Pages, you cannot rely on existing knowledge of SSG, for example, does Codeberg pages EVEN LOOK AT dist/ why is the .css empty? NOTHING IS STYLED WHEN YOU DONT RUN IT THROUGH VITE SERVER!!!

