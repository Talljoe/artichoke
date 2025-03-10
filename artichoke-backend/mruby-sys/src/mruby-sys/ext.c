// ext is partially derived from mrusty @ 1.0.0
// <https://github.com/anima-engine/mrusty/tree/v1.0.0>
//
// Copyright (C) 2016  Dragoș Tiselice
// Licensed under the Mozilla Public License 2.0

// ext is partially derived from go-mruby @ cd6a04a
// <https://github.com/mitchellh/go-mruby/tree/cd6a04a>
//
// Copyright (c) 2017 Mitchell Hashimoto
// Licensed under the MIT License

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#include <mruby.h>
#include <mruby/array.h>
#include <mruby/range.h>
#include <mruby/string.h>

#include <mruby-sys/ext.h>

// Check whether `mrb_value` is nil, false, or true

MRB_API _Bool mrb_sys_value_is_nil(mrb_value value) { return mrb_nil_p(value); }

MRB_API _Bool mrb_sys_value_is_false(mrb_value value) {
  return mrb_false_p(value);
}

MRB_API _Bool mrb_sys_value_is_true(mrb_value value) {
  return mrb_true_p(value);
}

MRB_API _Bool mrb_sys_range_excl(mrb_state *mrb, mrb_value value) {
  return mrb_range_excl_p(mrb, value);
}

MRB_API _Bool mrb_sys_obj_frozen(mrb_state *mrb, mrb_value value) {
  (void)(mrb);
  return mrb_immediate_p(value) || MRB_FROZEN_P(mrb_basic_ptr(value));
}

// Extract pointers from `mrb_value`s

MRB_API mrb_int mrb_sys_fixnum_to_cint(mrb_value value) {
  return mrb_fixnum(value);
}

MRB_API mrb_float mrb_sys_float_to_cdouble(mrb_value value) {
  return mrb_float(value);
}

MRB_API void *mrb_sys_cptr_ptr(mrb_value value) { return mrb_cptr(value); }

MRB_API struct RBasic *mrb_sys_basic_ptr(mrb_value value) {
  return mrb_basic_ptr(value);
}

MRB_API struct RObject *mrb_sys_obj_ptr(mrb_value value) {
  return mrb_obj_ptr(value);
}

MRB_API struct RProc *mrb_sys_proc_ptr(mrb_value value) {
  return mrb_proc_ptr(value);
}

MRB_API struct RClass *mrb_sys_class_ptr(mrb_value value) {
  return mrb_class_ptr(value);
}

MRB_API struct RClass *mrb_sys_class_to_rclass(mrb_value value) {
  return (struct RClass *)value.value.p;
}

MRB_API struct RClass *mrb_sys_class_of_value(struct mrb_state *mrb,
                                              mrb_value value) {
  return mrb_class(mrb, value);
}

// Construct `mrb_value`s

MRB_API mrb_value mrb_sys_nil_value(void) { return mrb_nil_value(); }

MRB_API mrb_value mrb_sys_false_value(void) { return mrb_false_value(); }

MRB_API mrb_value mrb_sys_true_value(void) { return mrb_true_value(); }

MRB_API mrb_value mrb_sys_fixnum_value(mrb_int value) {
  return mrb_fixnum_value(value);
}

MRB_API mrb_value mrb_sys_float_value(struct mrb_state *mrb, mrb_float value) {
  return mrb_float_value(mrb, value);
}

MRB_API mrb_value mrb_sys_cptr_value(struct mrb_state *mrb, void *ptr) {
  mrb_value value;
  (void)(mrb);

  SET_CPTR_VALUE(mrb, value, ptr);

  return value;
}

MRB_API mrb_value mrb_sys_obj_value(void *p) { return mrb_obj_value(p); }

MRB_API mrb_value mrb_sys_class_value(struct RClass *klass) {
  mrb_value value;

  value.value.p = klass;
  value.tt = MRB_TT_CLASS;

  return value;
}

MRB_API mrb_value mrb_sys_module_value(struct RClass *module) {
  mrb_value value;

  value.value.p = module;
  value.tt = MRB_TT_MODULE;

  return value;
}

MRB_API mrb_value mrb_sys_data_value(struct RData *data) {
  mrb_value value;

  value.value.p = data;
  value.tt = MRB_TT_DATA;

  return value;
}

MRB_API mrb_value mrb_sys_proc_value(struct mrb_state *mrb,
                                     struct RProc *proc) {
  mrb_value value = mrb_cptr_value(mrb, proc);

  value.tt = MRB_TT_PROC;

  return value;
}

// Manipulate `Symbol`s

MRB_API mrb_value mrb_sys_new_symbol(mrb_sym id) {
  mrb_value value;
  mrb_symbol(value) = id;
  value.tt = MRB_TT_SYMBOL;

  return value;
}

// Manage Rust-backed `mrb_value`s

MRB_API void mrb_sys_set_instance_tt(struct RClass *class,
                                     enum mrb_vtype type) {
  MRB_SET_INSTANCE_TT(class, type);
}

MRB_API void mrb_sys_data_init(mrb_value *value, void *ptr,
                               const mrb_data_type *type) {
  mrb_data_init(*value, ptr, type);
}

// Raise exceptions and debug info

MRB_API mrb_noreturn void mrb_sys_raise(struct mrb_state *mrb,
                                        const char *eclass, const char *msg) {
  mrb_raise(mrb, mrb_class_get(mrb, eclass), msg);
}

MRB_API void mrb_sys_raise_current_exception(struct mrb_state *mrb) {
  if (mrb->exc) {
    mrb_exc_raise(mrb, mrb_obj_value(mrb->exc));
  }
}

// Manipulate Array `mrb_value`s

MRB_API mrb_int mrb_sys_ary_len(mrb_value value) {
  return ARY_LEN(mrb_ary_ptr(value));
}

// Manage the mruby garbage collector (GC)

MRB_API int mrb_sys_gc_arena_save(mrb_state *mrb) {
  return mrb_gc_arena_save(mrb);
}

MRB_API void mrb_sys_gc_arena_restore(mrb_state *mrb, int arena_index) {
  mrb_gc_arena_restore(mrb, arena_index);
}

MRB_API _Bool mrb_sys_gc_disable(mrb_state *mrb) {
  mrb_gc *gc = &mrb->gc;
  _Bool was_enabled = !gc->disabled;
  gc->disabled = 1;
  return was_enabled;
}

MRB_API _Bool mrb_sys_gc_enable(mrb_state *mrb) {
  mrb_gc *gc = &mrb->gc;
  _Bool was_enabled = !gc->disabled;
  gc->disabled = 0;
  return was_enabled;
}

MRB_API _Bool mrb_sys_value_is_dead(mrb_state *mrb, mrb_value value) {
  // immediate values such as Fixnums and Symbols are never garbage
  // collected, so they are never dead. See `mrb_gc_protect` in gc.c.
  if (mrb_immediate_p(value)) {
    return FALSE;
  }

  struct RBasic *ptr = mrb_basic_ptr(value);

  if (ptr == NULL) {
    return TRUE;
  }

  return mrb_object_dead_p(mrb, ptr);
}

MRB_API int mrb_sys_gc_live_objects(mrb_state *mrb) {
  mrb_gc *gc = &mrb->gc;
  return gc->live;
}

MRB_API void mrb_sys_safe_gc_mark(mrb_state *mrb, mrb_value value) {
  if (!mrb_immediate_p(value)) {
    mrb_gc_mark(mrb, mrb_basic_ptr(value));
  }
}
