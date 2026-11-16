"""Exercise the shipped bridge actions, including workspace boundary wraparound."""

from pathlib import Path
import shutil
import subprocess
import unittest


@unittest.skipUnless(shutil.which("node"), "requires Node.js for the isolated bridge harness")
class GnomeWorkspaceActions(unittest.TestCase):
    def test_window_moves_wrap_and_follow_focus(self):
        extension = Path(__file__).resolve().parents[4] / "crates/tecloid-core/assets/gnome-extension/tecloid@bozonx.dev/extension.js"
        script = r'''
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(process.argv[1], 'utf8')
    .replace(/^import .*;$/gm, '')
    .replace('export default class Bridge', 'class Bridge');
for (const count of [1, 2, 4]) {
    let index = 0;
    let followed = null;
    const window = {
        get_workspace: () => workspaces[index],
        is_on_all_workspaces: () => false,
        change_workspace: workspace => { index = workspace.index(); },
    };
    const workspaces = Array.from({length: count}, (_, id) => ({
        index: () => id,
        activate_with_focus: (focused, timestamp) => {
            assert.equal(focused, window);
            assert.equal(timestamp, 123);
            followed = id;
        },
    }));
    const context = vm.createContext({
        Extension: class {},
        global: {
            display: {focus_window: window},
            workspace_manager: {
                n_workspaces: count,
                get_workspace_by_index: id => workspaces[id],
            },
            get_current_time: () => 123,
        },
    });
    vm.runInContext(source + '\nthis.bridge = new Bridge();', context);
    for (const [action, step] of [['moveWindowToNextDesktop', 1], ['moveWindowToPreviousDesktop', -1]]) {
        index = 0;
        for (let press = 0; press < count * 3; press++) {
            const expected = (index + step + count) % count;
            context.bridge._action(action);
            assert.equal(index, expected);
            assert.equal(followed, expected);
        }
    }
    window.is_on_all_workspaces = () => true;
    assert.throws(() => context.bridge._action('moveWindowToNextDesktop'), /all workspaces/);
    assert.throws(() => context.bridge._action('moveWindowToPreviousDesktop'), /all workspaces/);
}
'''
        subprocess.run(["node", "-e", script, str(extension)], check=True, capture_output=True, text=True)


if __name__ == "__main__":
    unittest.main()
