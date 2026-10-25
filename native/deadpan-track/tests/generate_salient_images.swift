// Original synthetic proposal fixture drawing. MIT, like the Deadpan project.
// No Vision request, external image, font or person footage is used.
import Foundation
import CoreVideo
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers
import CryptoKit

let output = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
let width = 640, height = 360

func hash(_ data: Data) -> String { SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined() }
func rect(_ r: CGRect) -> [String: Double] {
    ["x":r.origin.x, "y":r.origin.y, "width":r.width, "height":r.height]
}
func revisions(_ set: IndexSet) -> [Int] { Array(set) }
func fill(_ c: CGContext, _ color: CGColor, _ r: CGRect) { c.setFillColor(color); c.fill(r) }
func rgb(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat) -> CGColor { CGColor(red:r,green:g,blue:b,alpha:1) }
func draw(_ c: CGContext, _ name: String) {
    fill(c, rgb(0.84,0.87,0.88), CGRect(x:0,y:0,width:width,height:height))
    if name == "blank-control" { return }
    if name == "red-square" {
        fill(c,rgb(0.82,0.05,0.02),CGRect(x:370,y:95,width:120,height:140))
        return
    }
    if name == "shaded-orange" || name == "two-objects" {
        c.saveGState()
        c.setShadow(offset:CGSize(width:8,height:-10),blur:12,color:rgb(0.35,0.37,0.38))
        c.setFillColor(rgb(0.95,0.37,0.03))
        c.fillEllipse(in:CGRect(x:340,y:90,width:155,height:155))
        c.restoreGState()
        c.saveGState()
        c.addEllipse(in:CGRect(x:340,y:90,width:155,height:155))
        c.clip()
        let colors=[rgb(1,0.83,0.27),rgb(1,0.46,0.04),rgb(0.57,0.16,0.01)] as CFArray
        let gradient=CGGradient(colorsSpace:CGColorSpaceCreateDeviceRGB(),colors:colors,locations:[0,0.55,1])!
        c.drawRadialGradient(gradient,startCenter:CGPoint(x:385,y:205),startRadius:0,
            endCenter:CGPoint(x:420,y:170),endRadius:115,options:.drawsAfterEndLocation)
        c.restoreGState()
        c.setFillColor(rgb(0.12,0.42,0.10))
        c.fillEllipse(in:CGRect(x:409,y:232,width:55,height:19))
    }
    if name == "ceramic-mug" || name == "two-objects" {
        let x:CGFloat = name == "two-objects" ? 110 : 360
        c.setStrokeColor(rgb(0.1,0.27,0.60)); c.setLineWidth(17)
        c.strokeEllipse(in:CGRect(x:x+100,y:135,width:63,height:70))
        c.setFillColor(rgb(0.12,0.40,0.82))
        c.addPath(CGPath(roundedRect:CGRect(x:x,y:95,width:120,height:140),cornerWidth:15,cornerHeight:15,transform:nil))
        c.fillPath()
        c.setFillColor(rgb(0.08,0.15,0.30)); c.fillEllipse(in:CGRect(x:x+4,y:218,width:112,height:21))
        c.setFillColor(rgb(0.43,0.67,0.96)); c.fill(CGRect(x:x+12,y:110,width:15,height:100))
    }
    if name == "toy-house" {
        fill(c,rgb(0.87,0.59,0.21),CGRect(x:345,y:80,width:170,height:140))
        c.setFillColor(rgb(0.66,0.12,0.09)); c.beginPath()
        c.move(to:CGPoint(x:325,y:215));c.addLine(to:CGPoint(x:430,y:290));c.addLine(to:CGPoint(x:535,y:215));c.closePath();c.fillPath()
        fill(c,rgb(0.14,0.25,0.34),CGRect(x:411,y:80,width:44,height:80))
        fill(c,rgb(0.50,0.75,0.92),CGRect(x:361,y:154,width:32,height:36))
        fill(c,rgb(0.50,0.75,0.92),CGRect(x:474,y:154,width:26,height:36))
    }
}


var reports:[[String:Any]]=[]
for (name,file) in [("blank-control","salient-blank"),("ceramic-mug","salient-mug"),("two-objects","salient-two")] {
    var allocated:CVPixelBuffer?
    let status=CVPixelBufferCreate(kCFAllocatorDefault,width,height,kCVPixelFormatType_32BGRA,nil,&allocated)
    guard status==kCVReturnSuccess, let buffer=allocated else {fatalError("allocate fixture raster")}
    CVPixelBufferLockBaseAddress(buffer,[])
    guard let base=CVPixelBufferGetBaseAddress(buffer),
        let context=CGContext(data:base,width:width,height:height,bitsPerComponent:8,
            bytesPerRow:CVPixelBufferGetBytesPerRow(buffer),space:CGColorSpaceCreateDeviceRGB(),
            bitmapInfo:CGImageAlphaInfo.premultipliedFirst.rawValue|CGBitmapInfo.byteOrder32Little.rawValue)
        else {fatalError("construct fixture raster")}
    draw(context,name)
    let bytes=Data(bytes:base,count:CVPixelBufferGetBytesPerRow(buffer)*height)
    try bytes.write(to:output.appendingPathComponent(file+".bgra"))
    let png=output.appendingPathComponent(file+".png")
    guard let image=context.makeImage(),let destination=CGImageDestinationCreateWithURL(png as CFURL,UTType.png.identifier as CFString,1,nil)
        else {fatalError("make fixture PNG")}
    CGImageDestinationAddImage(destination,image,nil)
    guard CGImageDestinationFinalize(destination) else {fatalError("write fixture PNG")}
    CVPixelBufferUnlockBaseAddress(buffer,[])
    reports.append(["name":file,"width":width,"height":height,"bgra_sha256":hash(bytes),"png_sha256":hash(try Data(contentsOf:png))])
}
let data=try JSONSerialization.data(withJSONObject:reports,options:[.prettyPrinted,.sortedKeys])
try data.write(to:output.appendingPathComponent("images.json"))
