# coding=utf-8
# Copyright 2014 Pants project contributors (see CONTRIBUTORS.md).
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from __future__ import (absolute_import, division, generators, nested_scopes, print_function,
                        unicode_literals, with_statement)

import os

from pants.backend.jvm.tasks.classpath_products import ClasspathProducts
from pants.util.dirutil import safe_file_dump, safe_mkdir, safe_mkdtemp
from pants_test.jvm.nailgun_task_test_base import NailgunTaskTestBase


class JarTaskTestBase(NailgunTaskTestBase):
  """Prepares an ephemeral test build root that supports jar tasks."""

  def add_to_compile_classpath(self, context, tgt, files_dict):
    """Creates and adds the given files to the classpath for the given target under a temp path."""
    compile_classpath = context.products.get_data('compile_classpath', ClasspathProducts)
    # Create a temporary directory under the target id, then dump all files.
    target_dir = os.path.join(self.test_workdir, tgt.id)
    safe_mkdir(target_dir)
    classpath_dir = safe_mkdtemp(dir=target_dir)
    for rel_path, content in files_dict.items():
      safe_file_dump(os.path.join(classpath_dir, rel_path), content)
    # Add to the classpath.
    compile_classpath.add_for_target(tgt, [('default', classpath_dir)])
