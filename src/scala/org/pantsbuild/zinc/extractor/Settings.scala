/**
 * Copyright (C) 2012 Typesafe, Inc. <http://www.typesafe.com>
 */

package org.pantsbuild.zinc.extractor

import java.io.File

import org.pantsbuild.zinc.options.OptionSet
import org.pantsbuild.zinc.analysis.AnalysisOptions

/**
 * All parsed command-line options.
 */
case class Settings(
  help: Boolean             = false,
  analysis: AnalysisOptions = AnalysisOptions(),
  summaryJson: Option[File] = None
)

object Settings extends OptionSet[Settings] {
  override def empty = Settings()

  override val options = Seq(
    header("Output options:"),
    boolean(  ("-help", "-h"),                 "Print this usage message",
      (s: Settings) => s.copy(help = true)),
    file(      "-summary-json", "file",        "Output file to write an analysis summary to.",
      (s: Settings, f: File) => s.copy(summaryJson = Some(f))),

    header("Analysis options:"),
    file(      "-analysis-cache", "file",      "Cache file for compile analysis",
      (s: Settings, f: File) => s.copy(analysis = s.analysis.copy(_cache = Some(f)))),
    fileMap(   "-rebase-map",                  "Source and destination paths to rebase in persisted analysis (file:file,...)",
      (s: Settings, m: Map[File, File]) => s.copy(analysis = s.analysis.copy(rebaseMap = m)))
  )
}
