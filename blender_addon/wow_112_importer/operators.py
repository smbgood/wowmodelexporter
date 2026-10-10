import json
import os
import subprocess
import tempfile
import threading
from pathlib import Path

import bpy


def _parse_animation_ids(csv_value):
    ids = []
    for token in csv_value.split(","):
        token = token.strip()
        if not token:
            continue
        ids.append(int(token))
    return ids


class WOW112_OT_export_and_import(bpy.types.Operator):
    bl_idname = "wow112.export_and_import"
    bl_label = "Import WoW 1.12 Character"
    bl_options = {"REGISTER"}

    _thread = None
    _result = None
    _temp_dir = None

    def _build_config(self, context):
        settings = context.scene.wow112_settings
        prefs = bpy.context.preferences.addons[__package__].preferences

        output = Path(tempfile.gettempdir()) / "wow112_character.glb"
        config_path = Path(tempfile.gettempdir()) / "wow112_character_config.json"

        payload = {
            "wow_data_path": prefs.wow_data_path,
            "output_path": str(output),
            "profile": {
                "race": settings.race,
                "sex": settings.sex,
                "skin_index": settings.skin_index,
                "face_index": settings.face_index,
                "hair_style_index": settings.hair_style_index,
                "hair_color_index": settings.hair_color_index,
                "facial_hair_index": settings.facial_hair_index,
            },
            "equipment_display_ids": {
                "head": settings.head_display_id,
                "shoulder": settings.shoulder_display_id,
                "chest": settings.chest_display_id,
                "back": settings.back_display_id,
                "wrist": settings.wrist_display_id,
                "waist": settings.waist_display_id,
                "legs": settings.legs_display_id,
                "feet": settings.feet_display_id,
                "hands": settings.hands_display_id,
                "shirt": settings.shirt_display_id,
                "tabard": settings.tabard_display_id,
                "ranged": settings.ranged_display_id,
                "mainhand": settings.weapon_mainhand_display_id,
                "offhand": settings.weapon_offhand_display_id,
            },
            "animation_ids": _parse_animation_ids(settings.animation_ids_csv),
        }
        with config_path.open("w", encoding="utf-8") as handle:
            json.dump(payload, handle, indent=2)
        return config_path, output

    def execute(self, context):
        prefs = bpy.context.preferences.addons[__package__].preferences
        converter = prefs.converter_executable
        if not converter or not os.path.exists(converter):
            self.report({"ERROR"}, "Set Converter Executable in addon preferences.")
            return {"CANCELLED"}

        config_path, output_path = self._build_config(context)
        self._result = None

        def _run_export():
            try:
                run = subprocess.run(
                    [converter, "export", "--config", str(config_path)],
                    check=False,
                    capture_output=True,
                    text=True,
                )
                self._result = (run.returncode, run.stdout, run.stderr, output_path)
            except Exception as exc:  # pylint: disable=broad-except
                self._result = (-1, "", str(exc), output_path)

        self._thread = threading.Thread(target=_run_export, daemon=True)
        self._thread.start()
        context.window_manager.modal_handler_add(self)
        return {"RUNNING_MODAL"}

    def modal(self, context, event):
        if event.type != "TIMER":
            if self._thread and not self._thread.is_alive():
                return self._finalize(context)
            return {"PASS_THROUGH"}

        if self._thread and self._thread.is_alive():
            return {"RUNNING_MODAL"}
        return self._finalize(context)

    def invoke(self, context, event):
        wm = context.window_manager
        self._timer = wm.event_timer_add(0.2, window=context.window)
        result = self.execute(context)
        return result

    def _finalize(self, context):
        wm = context.window_manager
        if hasattr(self, "_timer"):
            wm.event_timer_remove(self._timer)

        code, stdout, stderr, output_path = self._result or (-1, "", "unknown error", None)
        if code != 0:
            msg = stderr.strip() or stdout.strip() or "export failed"
            self.report({"ERROR"}, msg[:512])
            return {"CANCELLED"}

        if not output_path or not output_path.exists():
            self.report({"ERROR"}, "Converter succeeded but output GLB is missing.")
            return {"CANCELLED"}

        bpy.ops.import_scene.gltf(filepath=str(output_path))
        imported_collection = bpy.data.collections.new("WoW112_Character")
        context.scene.collection.children.link(imported_collection)
        self.report({"INFO"}, "Imported WoW 1.12 character. Ready for posing.")
        return {"FINISHED"}
