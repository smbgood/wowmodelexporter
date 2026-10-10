bl_info = {
    "name": "WoW 1.12 Character Importer",
    "author": "wowmodelexport",
    "version": (0, 1, 0),
    "blender": (4, 0, 0),
    "location": "View3D > Sidebar > WoW 1.12",
    "description": "Imports WoW 1.12 characters through an external converter",
    "category": "Import-Export",
}

import bpy

from .operators import WOW112_OT_export_and_import
from .ui import WOW112_PT_panel


class WOW112Preferences(bpy.types.AddonPreferences):
    bl_idname = __package__

    converter_executable: bpy.props.StringProperty(
        name="Converter Executable",
        subtype="FILE_PATH",
        description="Path to wowmodelexport-converter executable",
    )

    wow_data_path: bpy.props.StringProperty(
        name="WoW Data Path",
        subtype="DIR_PATH",
        description="Path to World of Warcraft 1.12 Data directory",
    )

    def draw(self, context):
        layout = self.layout
        layout.prop(self, "converter_executable")
        layout.prop(self, "wow_data_path")


class WOW112CharacterSettings(bpy.types.PropertyGroup):
    race: bpy.props.EnumProperty(
        name="Race",
        items=[
            ("human", "Human", "Vanilla race"),
            ("orc", "Orc", "Vanilla race"),
            ("dwarf", "Dwarf", "Vanilla race"),
            ("nightelf", "Night Elf", "Vanilla race"),
            ("undead", "Undead", "Vanilla race"),
            ("tauren", "Tauren", "Vanilla race"),
            ("gnome", "Gnome", "Vanilla race"),
            ("troll", "Troll", "Vanilla race"),
            ("highelf", "High Elf (alias)", "Maps to Night Elf assets for Turtle compatibility"),
        ],
        default="human",
    )
    sex: bpy.props.EnumProperty(
        name="Sex",
        items=[("female", "Female", ""), ("male", "Male", "")],
        default="female",
    )
    skin_index: bpy.props.IntProperty(name="Skin", min=0, max=31, default=0)
    face_index: bpy.props.IntProperty(name="Face", min=0, max=31, default=0)
    hair_style_index: bpy.props.IntProperty(name="Hair Style", min=0, max=31, default=0)
    hair_color_index: bpy.props.IntProperty(name="Hair Color", min=0, max=31, default=0)
    facial_hair_index: bpy.props.IntProperty(name="Facial Hair", min=0, max=31, default=0)
    head_display_id: bpy.props.IntProperty(name="Head Display ID", min=0, default=0)
    shoulder_display_id: bpy.props.IntProperty(name="Shoulder Display ID", min=0, default=0)
    chest_display_id: bpy.props.IntProperty(name="Chest Display ID", min=0, default=0)
    back_display_id: bpy.props.IntProperty(name="Back Display ID", min=0, default=0)
    wrist_display_id: bpy.props.IntProperty(name="Wrist Display ID", min=0, default=0)
    waist_display_id: bpy.props.IntProperty(name="Waist Display ID", min=0, default=0)
    legs_display_id: bpy.props.IntProperty(name="Legs Display ID", min=0, default=0)
    feet_display_id: bpy.props.IntProperty(name="Feet Display ID", min=0, default=0)
    hands_display_id: bpy.props.IntProperty(name="Hands Display ID", min=0, default=0)
    tabard_display_id: bpy.props.IntProperty(name="Tabard Display ID", min=0, default=0)
    shirt_display_id: bpy.props.IntProperty(name="Shirt Display ID", min=0, default=0)
    ranged_display_id: bpy.props.IntProperty(name="Ranged Display ID", min=0, default=0)
    weapon_mainhand_display_id: bpy.props.IntProperty(name="Mainhand Display ID", min=0, default=0)
    weapon_offhand_display_id: bpy.props.IntProperty(name="Offhand Display ID", min=0, default=0)
    animation_ids_csv: bpy.props.StringProperty(name="Animation IDs", default="0,4,5,69")


classes = (
    WOW112Preferences,
    WOW112CharacterSettings,
    WOW112_OT_export_and_import,
    WOW112_PT_panel,
)


def register():
    for cls in classes:
        bpy.utils.register_class(cls)
    bpy.types.Scene.wow112_settings = bpy.props.PointerProperty(type=WOW112CharacterSettings)


def unregister():
    del bpy.types.Scene.wow112_settings
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
