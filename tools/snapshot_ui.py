import json
import subprocess
import tkinter as tk
from pathlib import Path
from tkinter import filedialog, messagebox, ttk


class SnapshotUI(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("WoW Snapshot Config Builder")
        self.geometry("980x620")

        self.converter_var = tk.StringVar(
            value=str(Path("converter/target/debug/wowmodelexport-converter.exe").resolve())
        )
        self.savedvars_var = tk.StringVar(
            value=r"B:\raven\RavenCraft\WTF\Account\YOUR_ACCOUNT\YOUR_REALM\YOUR_CHARACTER\SavedVariables\WowModelExporterSnapshot.lua"
        )
        self.wow_data_var = tk.StringVar(value=r"B:\raven\RavenCraft\Data")
        self.output_configs_var = tk.StringVar(
            value=str(Path("converter/generated_snapshot_configs").resolve())
        )
        self.output_glb_var = tk.StringVar(value=r"B:\raven\RavenCraft\exports")
        self.lookup_var = tk.StringVar(
            value=r"B:\raven\RavenCraft\item_display_lookup.json"
        )
        self.db_export_var = tk.StringVar(
            value=r"B:\raven\RavenCraft\db_item_template_export.sql"
        )
        self.use_item_ids_var = tk.BooleanVar(value=False)

        self.snapshots = []
        self._build()

    def _build(self):
        root = ttk.Frame(self, padding=10)
        root.pack(fill=tk.BOTH, expand=True)

        fields = [
            ("Converter EXE", self.converter_var, self._pick_file),
            ("SavedVariables LUA", self.savedvars_var, self._pick_file),
            ("WoW Data Path", self.wow_data_var, self._pick_dir),
            ("Output Config Dir", self.output_configs_var, self._pick_dir),
            ("Output GLB Dir", self.output_glb_var, self._pick_dir),
            ("DB Export Lookup (SQL/CSV)", self.db_export_var, self._pick_file),
            ("Item->Display Lookup (JSON/CSV)", self.lookup_var, self._pick_file),
        ]

        for i, (label, var, picker) in enumerate(fields):
            ttk.Label(root, text=label).grid(row=i, column=0, sticky="w", pady=4)
            entry = ttk.Entry(root, textvariable=var, width=95)
            entry.grid(row=i, column=1, sticky="ew", pady=4, padx=6)
            ttk.Button(
                root,
                text="Browse...",
                command=lambda v=var, p=picker: p(v),
            ).grid(row=i, column=2, sticky="ew", pady=4)

        ttk.Checkbutton(
            root,
            variable=self.use_item_ids_var,
            text="Copy item IDs into equipment_display_ids (optional fallback)",
        ).grid(row=7, column=0, columnspan=3, sticky="w", pady=6)

        button_row = ttk.Frame(root)
        button_row.grid(row=8, column=0, columnspan=3, sticky="w", pady=6)
        ttk.Button(button_row, text="Refresh Snapshots", command=self.refresh_snapshots).pack(
            side=tk.LEFT, padx=(0, 8)
        )
        ttk.Button(button_row, text="Generate Selected Config", command=self.generate_selected).pack(
            side=tk.LEFT, padx=(0, 8)
        )
        ttk.Button(button_row, text="Generate All Configs", command=self.generate_all).pack(
            side=tk.LEFT, padx=(0, 8)
        )
        ttk.Button(
            button_row,
            text="Generate Model from Config...",
            command=self.generate_model_from_config,
        ).pack(
            side=tk.LEFT
        )

        columns = ("id", "name", "realm", "race", "sex", "class", "gear_count")
        self.tree = ttk.Treeview(root, columns=columns, show="headings", height=18)
        for col in columns:
            self.tree.heading(col, text=col)
            self.tree.column(col, width=130 if col != "id" else 60, anchor=tk.W)
        self.tree.grid(row=9, column=0, columnspan=3, sticky="nsew", pady=(8, 4))

        ttk.Label(
            root,
            text="Tip: run /wmesnap in-game, then /reload or logout before refreshing here.",
        ).grid(row=10, column=0, columnspan=3, sticky="w", pady=4)

        self.log = tk.Text(root, height=8, wrap="word")
        self.log.grid(row=11, column=0, columnspan=3, sticky="nsew", pady=(6, 0))
        self.log.configure(state="disabled")

        root.columnconfigure(1, weight=1)
        root.rowconfigure(9, weight=1)
        root.rowconfigure(11, weight=0)

    def _pick_file(self, variable):
        current = Path(variable.get())
        path = filedialog.askopenfilename(initialdir=str(current.parent if current.parent.exists() else Path.cwd()))
        if path:
            variable.set(path)

    def _pick_dir(self, variable):
        current = Path(variable.get())
        path = filedialog.askdirectory(initialdir=str(current if current.exists() else Path.cwd()))
        if path:
            variable.set(path)

    def _log(self, text):
        self.log.configure(state="normal")
        self.log.insert(tk.END, text + "\n")
        self.log.see(tk.END)
        self.log.configure(state="disabled")

    def _run_converter(self, args):
        exe = Path(self.converter_var.get())
        if not exe.exists():
            raise RuntimeError(f"converter exe not found: {exe}")
        completed = subprocess.run(
            [str(exe), *args],
            text=True,
            capture_output=True,
            check=False,
        )
        raw = completed.stdout.strip() or completed.stderr.strip()
        if not raw:
            raise RuntimeError("converter returned no output")
        try:
            payload = json.loads(raw)
        except json.JSONDecodeError as exc:
            raise RuntimeError(f"failed to parse converter output: {raw}") from exc
        if completed.returncode != 0 or not payload.get("ok", False):
            raise RuntimeError(payload.get("error", raw))
        return payload

    def refresh_snapshots(self):
        try:
            payload = self._run_converter(
                [
                    "import-snapshots",
                    "--saved-variables",
                    self.savedvars_var.get(),
                    "--output-dir",
                    self.output_configs_var.get(),
                    "--wow-data-path",
                    self.wow_data_var.get(),
                    "--output-glb-dir",
                    self.output_glb_var.get(),
                    "--list",
                ]
            )
        except Exception as exc:  # pylint: disable=broad-except
            messagebox.showerror("Refresh failed", str(exc))
            self._log(f"[error] {exc}")
            return

        self.snapshots = payload.get("snapshots", [])
        for item in self.tree.get_children():
            self.tree.delete(item)
        for snap in self.snapshots:
            self.tree.insert(
                "",
                tk.END,
                values=(
                    snap.get("snapshot_id"),
                    snap.get("name", ""),
                    snap.get("realm", ""),
                    snap.get("race", ""),
                    snap.get("sex", ""),
                    snap.get("class_name", ""),
                    len(snap.get("gear_item_ids", {})),
                ),
            )
        self._log(f"[info] loaded {len(self.snapshots)} snapshot(s)")

    def _selected_snapshot_id(self):
        selected = self.tree.selection()
        if not selected:
            return None
        row = self.tree.item(selected[0], "values")
        if not row:
            return None
        return int(row[0])

    def generate_selected(self):
        snapshot_id = self._selected_snapshot_id()
        if snapshot_id is None:
            messagebox.showwarning("No selection", "Select a snapshot first.")
            return
        self._generate(snapshot_id=snapshot_id)

    def generate_all(self):
        self._generate(snapshot_id=None)

    def generate_model_from_config(self):
        config_dir = Path(self.output_configs_var.get())
        config_path = filedialog.askopenfilename(
            title="Select exported model config",
            initialdir=str(config_dir if config_dir.exists() else Path.cwd()),
            filetypes=(("JSON config", "*.json"), ("All files", "*.*")),
        )
        if not config_path:
            return

        try:
            payload = self._run_converter(["export", "--config", config_path])
        except Exception as exc:  # pylint: disable=broad-except
            messagebox.showerror("Model generation failed", str(exc))
            self._log(f"[error] {exc}")
            return

        output_path = payload.get("output_path", "")
        self._log(
            f"[ok] generated model: {output_path} "
            f"({payload.get('output_size_bytes', 0)} bytes, "
            f"{payload.get('geoset_count', 0)} geosets, "
            f"{payload.get('animation_count', 0)} animations)"
        )

    def _generate(self, snapshot_id):
        args = [
            "import-snapshots",
            "--saved-variables",
            self.savedvars_var.get(),
            "--output-dir",
            self.output_configs_var.get(),
            "--wow-data-path",
            self.wow_data_var.get(),
            "--output-glb-dir",
            self.output_glb_var.get(),
        ]
        if snapshot_id is not None:
            args.extend(["--snapshot-id", str(snapshot_id)])
        db_export_path = self.db_export_var.get().strip()
        if db_export_path:
            args.extend(["--db-export-lookup", db_export_path])
        lookup_path = self.lookup_var.get().strip()
        if lookup_path:
            args.extend(["--item-display-lookup", lookup_path])
        if self.use_item_ids_var.get():
            args.append("--use-item-ids-as-display-ids")

        try:
            payload = self._run_converter(args)
        except Exception as exc:  # pylint: disable=broad-except
            messagebox.showerror("Generation failed", str(exc))
            self._log(f"[error] {exc}")
            return

        generated = payload.get("generated", [])
        self._log(
            f"[ok] generated {len(generated)} config(s), resolved slots={payload.get('resolved_slots', 0)}, unresolved slots={payload.get('unresolved_slots', 0)}"
        )
        for file_path in generated:
            self._log(f"  - {file_path}")


if __name__ == "__main__":
    app = SnapshotUI()
    app.mainloop()
