import CoreGraphics
// wid OWNER : print the window id of OWNER's main window
let owner = CommandLine.arguments[1]
let l = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
for w in l where (w["kCGWindowOwnerName"] as? String) == owner && (w["kCGWindowLayer"] as? Int) == 0 {
  let b = w["kCGWindowBounds"] as! [String: Any]
  print(w["kCGWindowNumber"]!, b["X"]!, b["Y"]!, b["Width"]!, b["Height"]!); break
}
