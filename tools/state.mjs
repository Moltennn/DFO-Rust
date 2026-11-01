import os from 'node:os';
import path from 'node:path';

/** The shell's local state: where tools/run.sh keeps its pid/log and the
 *  running shell writes its base URL. DFO_STATE_DIR overrides; otherwise
 *  ~/.local/share/dfo-rust (XDG_DATA_HOME respected). */
export const STATE =
  process.env.DFO_STATE_DIR ||
  path.join(
    process.env.XDG_DATA_HOME || path.join(os.homedir(), '.local/share'),
    'dfo-rust',
  );

export const URL_FILE = path.join(STATE, 'url');
