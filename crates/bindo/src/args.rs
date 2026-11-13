//! Match what the user typed to a recipe's arguments. Every value is checked
//! here, before anything runs; defaults fill the gaps.

use crate::recipe::{Arg, Recipe};

/// The values for `$1`, `$2`, …, or what is wrong with `given`.
pub fn bind(recipe: &Recipe, given: &[String]) -> Result<Vec<String>, String> {
    let takes_rest = recipe.args.last().is_some_and(|a| a.rest);
    if !takes_rest && given.len() > recipe.args.len() {
        let extra = &given[recipe.args.len()..];
        return Err(match recipe.args.len() {
            0 => format!("`{}` takes no arguments (got `{}`)", recipe.name, extra.join(" ")),
            n => format!("`{}` takes at most {n} argument(s); `{}` is extra", recipe.name, extra.join(" ")),
        });
    }
    let mut values = Vec::new();
    for (i, a) in recipe.args.iter().enumerate() {
        if a.rest {
            for v in given.iter().skip(i) {
                check(a, v)?;
                values.push(v.clone());
            }
            break;
        }
        let v = match (given.get(i), &a.default) {
            (Some(v), _) => v.clone(),
            (None, Some(d)) => d.clone(),
            (None, None) => return Err(format!("`{}` needs <{}>{}", recipe.name, a.name, expect(a))),
        };
        check(a, &v)?;
        values.push(v);
    }
    Ok(values)
}

fn check(a: &Arg, v: &str) -> Result<(), String> {
    if a.accepts(v) { Ok(()) } else { Err(format!("`{v}` is not a valid <{}>{}", a.name, expect(a))) }
}

/// ": one of on, off" / ": a number".
fn expect(a: &Arg) -> String {
    if a.choices.is_empty() { format!(": {}", a.kind.describe()) } else { format!(": one of {}", a.choices.join(", ")) }
}

/// Values the shell can offer for argument `index` (from 0) of `recipe`.
pub fn choices(recipe: &Recipe, index: usize) -> Vec<String> {
    let arg = recipe.args.get(index).or_else(|| recipe.args.last().filter(|a| a.rest));
    arg.map(|a| a.choices.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{Origin, parse};

    fn recipe(args: &str) -> Recipe {
        let text = format!(
            "[[recipe]]\nname = \"r\"\ncategory = \"c\"\ndescription = \"d\"\nrun = \"true\"\nargs = [{args}]\n"
        );
        parse(&text, &Origin::Shipped).unwrap().remove(0)
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn defaults_fill_missing_values() {
        let r = recipe("{ name = \"keep\", kind = \"number\", default = \"2\" }");
        assert_eq!(bind(&r, &[]).unwrap(), ["2"]);
        assert_eq!(bind(&r, &s(&["5"])).unwrap(), ["5"]);
        assert!(bind(&r, &s(&["five"])).unwrap_err().contains("a number"));
    }

    #[test]
    fn required_and_extra_arguments_are_reported() {
        let r = recipe("{ name = \"state\", choices = [\"on\", \"off\"] }");
        assert!(bind(&r, &[]).unwrap_err().contains("needs <state>: one of on, off"));
        assert!(bind(&r, &s(&["on", "now"])).unwrap_err().contains("`now` is extra"));
        assert!(bind(&r, &s(&["maybe"])).is_err());
        assert_eq!(bind(&r, &s(&["off"])).unwrap(), ["off"]);
    }

    #[test]
    fn options_never_reach_a_script() {
        let r = recipe("{ name = \"unit\" }, { name = \"more\", kind = \"text\", rest = true }");
        assert!(bind(&r, &s(&["--root=/"])).is_err());
        assert!(bind(&r, &s(&["sshd", "ok", "-x"])).is_err());
        assert_eq!(bind(&r, &s(&["sshd", "a b", "c"])).unwrap(), ["sshd", "a b", "c"]);
        assert_eq!(bind(&r, &s(&["sshd"])).unwrap(), ["sshd"]);
    }

    #[test]
    fn no_arguments_means_none() {
        let text = "[[recipe]]\nname = \"r\"\ncategory = \"c\"\ndescription = \"d\"\nrun = \"true\"\n";
        let r = parse(text, &Origin::Shipped).unwrap().remove(0);
        assert!(bind(&r, &s(&["x"])).unwrap_err().contains("takes no arguments"));
    }

    #[test]
    fn completion_offers_choices() {
        let r = recipe("{ name = \"state\", choices = [\"on\", \"off\"] }");
        assert_eq!(choices(&r, 0), ["on", "off"]);
        assert!(choices(&r, 1).is_empty());
    }
}
