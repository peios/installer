"""Keep the installer tools statically linked in every build configuration."""

def _musl_configuration(_settings, _attr):
    return {"//command_line_option:platforms": "//build:musl-platform"}

_musl_transition = transition(
    implementation = _musl_configuration,
    inputs = [],
    outputs = ["//command_line_option:platforms"],
)

def _artifact_impl(ctx):
    target = ctx.attr.binary[0]
    return [DefaultInfo(files = target[DefaultInfo].files, runfiles = target[DefaultInfo].default_runfiles)]

musl_artifact = rule(
    implementation = _artifact_impl,
    attrs = {
        "binary": attr.label(mandatory = True, cfg = _musl_transition),
        "_allowlist_function_transition": attr.label(default = "@bazel_tools//tools/allowlists/function_transition_allowlist"),
    },
    doc = "Expose a binary compiled with the static musl toolchain.",
)

def _test_impl(ctx):
    target = ctx.attr.binary[0]
    executable = ctx.actions.declare_file(ctx.label.name)
    ctx.actions.symlink(output = executable, target_file = target[DefaultInfo].files_to_run.executable, is_executable = True)
    return [DefaultInfo(executable = executable, runfiles = target[DefaultInfo].default_runfiles)]

musl_test = rule(
    implementation = _test_impl,
    attrs = {
        "binary": attr.label(mandatory = True, cfg = _musl_transition),
        "_allowlist_function_transition": attr.label(default = "@bazel_tools//tools/allowlists/function_transition_allowlist"),
    },
    test = True,
    doc = "Execute the installer regression suite under the static musl configuration.",
)
