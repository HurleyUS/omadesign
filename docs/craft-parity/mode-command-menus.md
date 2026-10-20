# Mode command menus

The Layout, Animation, Develop and Effect top menus were panel shortcuts only. This slice starts with Motion.

The Motion Animation menu now runs the real timeline commands. Add keyframe keys the selection at the playhead with the timeline's default ease. Add keyframe with ease offers Linear, Ease-in, Ease-out, Ease-in-out and Hold. Motion presets applies any of the 13 existing presets through the same command as the inspector, with the same per-object compatibility check. Loop playback flips the document loop flag. Each choice is one history step, Undo restores the exact saved bytes, Redo restores the edit, and the saved document reopens with the same motion data. A locked, hidden or missing target, an empty selection, an open file dialog or a frozen update disables the keyframe and preset choices and changes nothing. The timeline, keyframe, composition and 3D panel shortcuts stay below a separator.

At `9414ce96f7eb88cbe15b882746bb9dc857e17679` that route passed 6 tests. Receipt `d847335bb67926a1d8663d3e152f0a35f06950d0e6009f9152af5a6d151bf229`.

The Layout menu now runs the real frame and component commands. Wrap in frame puts the selection in a new frame on its layer. Add auto layout wraps the selection in a stacked frame, or adds a stack to a selected frame. Create component turns the selected frame into a main component and is disabled once it already is one. Detach instance turns the selected instance back into ordinary editable objects. Each choice is one history step, Undo restores the exact saved bytes, Redo restores the edit, and the document reopens with the same layout data. A locked, hidden or missing target, an empty selection, a selection across layers, an open file dialog or a frozen update disables these choices and changes nothing. The layout, hierarchy, component, variable and prototype panel shortcuts stay below a separator.

At `83b6e6ef9bd2b64ee6b2689de2fb62ea829c99d6` that route passed 6 tests. Receipt `ec4e4481c2e0ffea89f0c25e6ad90d3f37e856ffe0d3645fc59d34ec8e0f2ea3`.

The Photo Develop menu now runs real develop commands on the selected photo. Auto light balances exposure and contrast with the existing auto tone. Clear crop removes the crop and keeps every other setting. Reset all adjustments returns the photo to its default develop settings. Each of these is one photo history step: Undo restores the exact previous settings, Redo restores the edit, and the settings round-trip through their saved form. Show original only changes the view and records no history. With no selected photo, or while a batch or save is running, the commands are disabled and nothing changes; Clear crop and Reset are also disabled when there is nothing to clear. The develop, histogram, preset and history panel shortcuts stay below a separator.

At `1ba6b440a221476745156842e473895f4be8de69` that route passed 5 tests. Receipt `33af580461f2dde803a6aa3eb8d0dd7ee1336db21568f94e9c06eeb709615a86`.

The Design Effect menu now edits object effects on the selection. Add effect offers every entry in the existing effect catalog and appends it to each selected object. Clear effects removes the object effects from each selected object and is disabled when there are none. Each choice is one history step for the whole selection: Undo restores the exact saved bytes, Redo restores the edit, and the document reopens with the same effects. Locked, hidden and guide objects, pixel layers and objects on locked layers are skipped; when nothing remains, or a file dialog is open or updates are frozen, the choices are disabled and nothing changes. Layer effects stay in the Effects panel. The effects, adjustments, appearance and transparency panel shortcuts stay below a separator.

At `52c0cc5374d8eaab12468670ec3007bd140f08db` that route passed 5 tests. Receipt `2209a1393cc7ae93a77cb303607fa74a3c87cd7c92bf1bbabb0089f74bb40e2b`.

The Develop panel's Auto light button now records one photo history step, the same as the menu route; a second press is a separate step. Auto light also keeps whites and blacks inside the -1 to 1 range used by the sliders, the renderer and saved settings. Before, it could store values outside that range, and the panel sliders then rewrote them outside history. Rendered output is unchanged, because rendering already clamps to that range.

At `05ab6922ae96332ec61186dd490d74b17c33d01c` that route passed 7 tests. Receipt `cef55d6eedc7214dc95a662d081c456369b81ad8f96dd68ea819cffd67c99626`.

All four mode menus (Animation, Layout, Develop, Effect) now run real commands. No model, schema, dependency or version change.
