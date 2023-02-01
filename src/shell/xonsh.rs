use std::env;
use std::path::Path;

use indoc::formatdoc;

use crate::shell::{is_dir_in_path, Shell};

#[derive(Default)]
pub struct Xonsh {}

use std::borrow::Cow;
fn xonsh_escape_sq(input: &str) -> Cow<str> {
    for (i, ch) in input.chars().enumerate() {
        if xonsh_escape_char(ch).is_some() {
            let mut escaped_string = String::with_capacity(input.len());

            escaped_string.push_str(&input[..i]);
            for ch in input[i..].chars() {
                match xonsh_escape_char(ch) {
                    Some(escaped_char) => escaped_string.push_str(escaped_char),
                    None               => escaped_string.push(ch),
                };
            }
            return Cow::Owned(escaped_string);
        }
    }
    Cow::Borrowed(input)
}

fn xonsh_escape_char(ch: char) -> Option<&'static str> {
    match ch {
        '\'' => Some("\\'"),
        '\\' => Some("\\\\"),
        '\n' => Some("\\n"),
        _    => None,
    }
}

impl Shell for Xonsh {
    fn activate(&self, exe: &Path) -> String {
        let dir = exe.parent().unwrap();
        let exe = exe.display();
        let mut out = String::new();

        if !is_dir_in_path(dir) {
            let dir_str = dir.to_string_lossy();
            let dir_esc = xonsh_escape_sq(&dir_str);
            out.push_str(&formatdoc! {r#"
            from os import environ
            environ['PATH'] += ':' + '{dir_esc}'
            "#}); // todo: xonsh doesn't update the environment that rtx relies on with $PATH.add even with $UPDATE_OS_ENVIRON (github.com/xonsh/xonsh/issues/3207)
        }
        // using subprocess is a bit more complicated, but allows for using in pure .py configs, which start faster due to being compiled to .pyc
        out.push_str(&formatdoc! {r#"
            import subprocess
            from xonsh.built_ins  import XSH

            def listen_prompt(): # Hook Events
              ctx = XSH.ctx

              rtx_init_proc  = subprocess.run(["{exe}",'hook-env','-s','xonsh'],capture_output=True)
              rtx_init       = rtx_init_proc.stdout
              rtx_init_err   = rtx_init_proc.stderr

              if rtx_init_err:
                print(rtx_init_err.decode())
                return
              if rtx_init:
                execx(rtx_init.decode(), 'exec', ctx, filename='rtx')

            XSH.builtins.events.on_pre_prompt(listen_prompt) # Activate hook: before showing the prompt
            "#});

        out
    }

    fn deactivate(&self) -> String {
        formatdoc! {r#"
            from xonsh.built_ins  import XSH

            hooks = {{
              'on_pre_prompt' : ['listen_prompt'],
            }}
            for   hook_type in hooks:
              hook_fns = hooks[hook_type]
              for hook_fn   in hook_fns:
                hndl = getattr(XSH.builtins.events, hook_type)
                for fn in hndl:
                  if fn.__name__ == hook_fn:
                    hndl.remove(fn)
                    break
        "#}
    }

    fn set_env(&self, k: &str, v: &str) -> String {
        // PATH vars are lists, not colon:sep:string
        let start_path: [&str; 3] = ["PATH", "MANPATH", "INFOPATH"];
        let k_up = k.to_uppercase();
        if start_path.iter().any(|&s| s == k_up) {
            let mut v_out:String = String::with_capacity(v.len());
            v_out += "[";
            v_out += &env::split_paths(&v)
                .map(|p|
                    "'".to_owned() // ←'quote'
                    + &xonsh_escape_sq(&p.to_string_lossy())
                    +"'" ) // ↑ escape ' \ ␤ (docs.python.org/3/reference/lexical_analysis.html#strings)
                .collect::<Vec<_>>()
                .join(",");
            v_out += "]";
            format!(
                "${k}={v_out}\n",
                k = shell_escape::unix::escape(k.into())
                // todo: ↑ ↓ illegal chars should be dropped, not escaped?
            )
        } else {
            format!(
                "${k}='{v}'\n",
                k = shell_escape::unix::escape(k.into()),
                v = xonsh_escape_sq(v.into())
            )
        }
    }

    fn unset_env(&self, k: &str) -> String {
        format!("del ${k}\n", k = shell_escape::unix::escape(k.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_init() {
        insta::assert_snapshot!(Xonsh::default().activate(Path::new("/some/dir/rtx")));
    }

    #[test]
    fn test_set_env() {
        insta::assert_snapshot!(Xonsh::default().set_env("FOO", "1"));
    }

    #[test]
    fn test_unset_env() {
        insta::assert_snapshot!(Xonsh::default().unset_env("FOO"));
    }
}
