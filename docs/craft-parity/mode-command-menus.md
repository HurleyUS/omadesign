# Mode command menus

The Layout, Animation, Develop and Effect top menus were panel shortcuts only. This slice starts with Motion.

The Motion Animation menu now runs the real timeline commands. Add keyframe keys the selection at the playhead with the timeline's default ease. Add keyframe with ease offers Linear, Ease-in, Ease-out, Ease-in-out and Hold. Motion presets applies any of the 13 existing presets through the same command as the inspector, with the same per-object compatibility check. Loop playback flips the document loop flag. Each choice is one history step, Undo restores the exact saved bytes, Redo restores the edit, and the saved document reopens with the same motion data. A locked, hidden or missing target, an empty selection, an open file dialog or a frozen update disables the keyframe and preset choices and changes nothing. The timeline, keyframe, composition and 3D panel shortcuts stay below a separator.

At `9414ce96f7eb88cbe15b882746bb9dc857e17679` that route passed 6 tests. Receipt `d847335bb67926a1d8663d3e152f0a35f06950d0e6009f9152af5a6d151bf229`.

Layout, Develop and Effect remain panel shortcuts. No model, schema, dependency or version change.
