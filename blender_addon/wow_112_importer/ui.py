import bpy


class WOW112_PT_panel(bpy.types.Panel):
    bl_label = "WoW 1.12 Character"
    bl_idname = "WOW112_PT_panel"
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "WoW 1.12"

    def draw(self, context):
        layout = self.layout
        settings = context.scene.wow112_settings

        layout.label(text="Target adapter: Vanilla races + Turtle alias", icon="INFO")

        box = layout.box()
        box.label(text="Character")
        box.prop(settings, "race")
        box.prop(settings, "sex")
        box.prop(settings, "skin_index")
        box.prop(settings, "face_index")
        box.prop(settings, "hair_style_index")
        box.prop(settings, "hair_color_index")
        box.prop(settings, "facial_hair_index")

        equip = layout.box()
        equip.label(text="Equipment Display IDs")
        equip.prop(settings, "head_display_id")
        equip.prop(settings, "shoulder_display_id")
        equip.prop(settings, "chest_display_id")
        equip.prop(settings, "back_display_id")
        equip.prop(settings, "wrist_display_id")
        equip.prop(settings, "waist_display_id")
        equip.prop(settings, "legs_display_id")
        equip.prop(settings, "feet_display_id")
        equip.prop(settings, "hands_display_id")
        equip.prop(settings, "shirt_display_id")
        equip.prop(settings, "tabard_display_id")
        equip.prop(settings, "ranged_display_id")
        equip.prop(settings, "weapon_mainhand_display_id")
        equip.prop(settings, "weapon_offhand_display_id")

        layout.prop(settings, "animation_ids_csv")
        layout.operator("wow112.export_and_import", icon="ARMATURE_DATA")
