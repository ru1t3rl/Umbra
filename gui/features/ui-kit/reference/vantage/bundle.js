/* @ds-bundle: {"format":4,"namespace":"Vantage","components":[{"name":"Button"},{"name":"Chip"},{"name":"Panel"},{"name":"Card"},{"name":"TextField"},{"name":"Toggle"},{"name":"Slider"},{"name":"Tabs"},{"name":"CommandBar"},{"name":"Readout"},{"name":"Badge"},{"name":"AffiliationTag"},{"name":"EntityRow"},{"name":"Alert"}]} */
(function () {
  var R = window.React;
  var h = R.createElement;
  var cx = function () { return Array.prototype.slice.call(arguments).filter(Boolean).join(" "); };

  function Icon(props) {
    return h("span", { className: "vt-icon", "aria-hidden": "true" }, props.name);
  }

  function Button(props) {
    var variant = props.variant || "tonal", size = props.size, icon = props.icon, children = props.children, className = props.className;
    var rest = Object.assign({}, props); delete rest.variant; delete rest.size; delete rest.icon; delete rest.className; delete rest.children;
    var iconOnly = icon && !children;
    return h("button", Object.assign({ type: "button" }, rest, {
      className: cx("vt-btn", "vt-btn-" + variant, size === "sm" && "vt-btn-sm", iconOnly && "vt-btn-icon", className)
    }), icon ? h(Icon, { name: icon }) : null, children);
  }

  function Chip(props) {
    var selected = !!props.selected, glass = props.glass, icon = props.icon, children = props.children, className = props.className;
    var rest = Object.assign({}, props); delete rest.selected; delete rest.glass; delete rest.icon; delete rest.className; delete rest.children;
    return h("button", Object.assign({ type: "button", "aria-pressed": selected ? "true" : "false" }, rest, {
      className: cx("vt-chip", glass && "vt-chip-glass", className)
    }), icon ? h(Icon, { name: icon }) : null, children);
  }

  function ChipGroup(props) {
    return h("div", { className: cx("vt-chipgroup", props.className), role: "group", "aria-label": props.label }, props.children);
  }

  function Panel(props) {
    return h("section", { className: cx("vt-panel", props.className), style: props.width ? { width: props.width } : undefined },
      h("header", { className: "vt-panel-head" },
        h("h2", { className: "vt-panel-title" }, props.title),
        props.actions),
      h("div", { className: "vt-panel-body" }, props.children));
  }

  function Card(props) {
    var level = props.level || 2;
    return h("div", { className: cx("vt-card", level === 1 && "vt-card-1", level === 3 && "vt-card-3", props.variant === "outline" && "vt-card-outline", props.className) },
      props.title ? h("h3", { className: "vt-card-title" }, props.title) : null,
      props.subtitle ? h("p", { className: "vt-card-sub" }, props.subtitle) : null,
      props.children);
  }

  function TextField(props) {
    var id = props.id || ("vt-f-" + Math.random().toString(36).slice(2, 8));
    var rest = Object.assign({}, props); delete rest.label; delete rest.help; delete rest.mono; delete rest.invalid; delete rest.className;
    return h("label", { className: cx("vt-field", props.mono && "vt-field-mono", props.invalid && "vt-field-invalid", props.className), htmlFor: id },
      h("span", { className: "vt-label" }, props.label),
      h("input", Object.assign({ id: id, "aria-invalid": props.invalid ? "true" : undefined }, rest)),
      props.help ? h("span", { className: "vt-help" }, props.help) : null);
  }

  function Toggle(props) {
    var checked = !!props.checked;
    return h("button", { type: "button", role: "switch", "aria-checked": checked ? "true" : "false", className: cx("vt-toggle", props.className), disabled: props.disabled,
      onClick: function () { if (props.onChange) props.onChange(!checked); } },
      h("span", { className: "vt-toggle-track" }, h("span", { className: "vt-toggle-thumb" })),
      props.label ? h("span", null, props.label) : null);
  }

  function Slider(props) {
    var rest = Object.assign({}, props); delete rest.label; delete rest.format; delete rest.className;
    var fmt = props.format || function (v) { return String(v); };
    return h("div", { className: cx("vt-slider-wrap", props.className) },
      h("div", { className: "vt-slider-head" },
        h("span", { className: "vt-label" }, props.label),
        h("span", { className: "vt-slider-value" }, fmt(props.value))),
      h("input", Object.assign({ type: "range", className: "vt-slider", "aria-label": props.label }, rest)));
  }

  function Tabs(props) {
    var items = props.items || [], value = props.value;
    return h("div", { className: cx("vt-tabs", props.className), role: "tablist" }, items.map(function (it) {
      var selected = it.value === value;
      return h("button", { key: it.value, type: "button", role: "tab", "aria-selected": selected ? "true" : "false", className: "vt-tab",
        onClick: function () { if (props.onChange) props.onChange(it.value); } },
        it.label, it.count != null ? h("span", { className: "vt-tab-count" }, it.count) : null);
    }));
  }

  function CommandBar(props) {
    return h("div", { className: cx("vt-cmdbar", props.floating && "vt-cmdbar-floating", props.className) },
      h("span", { className: "vt-cmdbar-brand" }, props.brand || "Vantage"),
      h("div", { className: "vt-cmdbar-title" }, h("b", null, props.title), props.subtitle ? h("span", null, props.subtitle) : null),
      props.clock ? h("span", { className: cx("vt-cmdbar-clock", props.paused && "is-paused"), "aria-label": "Scenario clock" }, props.clock) : null,
      h("div", { className: "vt-cmdbar-actions" }, props.children));
  }

  function Readout(props) {
    var trend = props.trend;
    return h("div", { className: cx("vt-readout", props.live && "vt-readout-live", props.className) },
      h("span", { className: "vt-label" }, props.label),
      h("span", { className: "vt-readout-value" }, props.value, props.unit ? h("span", { className: "vt-readout-unit" }, props.unit) : null),
      props.delta ? h("span", { className: cx("vt-readout-delta", trend === "up" && "is-up", trend === "down" && "is-down") }, props.delta) : null);
  }

  function Badge(props) {
    return h("span", { className: cx("vt-badge", props.tone && props.tone !== "neutral" && "vt-badge-" + props.tone, props.className) }, props.children);
  }

  var FRAMES = {
    friend: h("rect", { className: "vt-affil-fill vt-affil-frame-shape", x: 2, y: 4, width: 16, height: 12, rx: 1 }),
    hostile: h("path", { className: "vt-affil-fill", d: "M10 1.5 L18.5 10 L10 18.5 L1.5 10 Z" }),
    neutral: h("rect", { className: "vt-affil-fill", x: 3, y: 3, width: 14, height: 14 }),
    unknown: h("path", { className: "vt-affil-fill", d: "M6 2.5 h8 a4 4 0 0 1 4 4 v8 a4 4 0 0 1 -4 4 h-8 a4 4 0 0 1 -4 -4 v-8 a4 4 0 0 1 4 -4 z" })
  };
  var FRAME_OUTLINES = {
    friend: h("rect", { className: "vt-affil-frame", x: 2, y: 4, width: 16, height: 12, rx: 1 }),
    hostile: h("path", { className: "vt-affil-frame", d: "M10 1.5 L18.5 10 L10 18.5 L1.5 10 Z" }),
    neutral: h("rect", { className: "vt-affil-frame", x: 3, y: 3, width: 14, height: 14 }),
    unknown: h("path", { className: "vt-affil-frame", d: "M6 2.5 h8 a4 4 0 0 1 4 4 v8 a4 4 0 0 1 -4 4 h-8 a4 4 0 0 1 -4 -4 v-8 a4 4 0 0 1 4 -4 z" })
  };
  var AFFIL_LABEL = { friend: "Friend", hostile: "Hostile", neutral: "Neutral", unknown: "Unknown" };

  function AffiliationTag(props) {
    var a = props.affiliation || "unknown";
    return h("span", { className: cx("vt-affil", "vt-affil-" + a, props.className), title: AFFIL_LABEL[a] },
      h("svg", { width: 20, height: 20, viewBox: "0 0 20 20", "aria-hidden": "true" }, FRAMES[a], FRAME_OUTLINES[a]),
      props.children || AFFIL_LABEL[a]);
  }

  function EntityRow(props) {
    return h("button", { type: "button", role: "option", "aria-selected": props.selected ? "true" : "false",
      className: cx("vt-entity", props.stale && "vt-entity-stale", props.className), onClick: props.onSelect },
      h(AffiliationTag, { affiliation: props.affiliation }, props.echelon || " "),
      h("span", null,
        h("span", { className: "vt-entity-name" }, props.name),
        props.id ? h("span", { className: "vt-entity-id" }, props.id) : null,
        props.meta ? h("div", { className: "vt-entity-meta" }, props.meta) : null),
      props.status ? h(Badge, { tone: props.status.tone }, props.status.label) : null);
  }

  function EntityList(props) {
    return h("div", { className: cx("vt-list", props.className), role: "listbox", "aria-label": props.label }, props.children);
  }

  var ALERT_GLYPH = { warning: "!", danger: "×", success: "✓", info: "i" };
  function Alert(props) {
    var tone = props.tone || "info";
    return h("div", { className: cx("vt-alert", "vt-alert-" + tone, props.className), role: tone === "danger" || tone === "warning" ? "alert" : "status" },
      h("span", { className: "vt-alert-icon", "aria-hidden": "true" }, ALERT_GLYPH[tone]),
      h("div", { className: "vt-alert-text" },
        h("div", { className: "vt-alert-title" }, props.title),
        props.children ? h("div", { className: "vt-alert-body" }, props.children) : null),
      props.action);
  }

  window.Vantage = {
    Button: Button, Chip: Chip, ChipGroup: ChipGroup, Panel: Panel, Card: Card, TextField: TextField, Toggle: Toggle,
    Slider: Slider, Tabs: Tabs, CommandBar: CommandBar, Readout: Readout, Badge: Badge, AffiliationTag: AffiliationTag,
    EntityRow: EntityRow, EntityList: EntityList, Alert: Alert, Icon: Icon
  };
})();
