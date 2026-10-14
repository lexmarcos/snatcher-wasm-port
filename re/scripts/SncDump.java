// Post-analysis script: re-apply labels, then dump function list, decompilation and listing.
// args: <outdir> [<labels-file>]
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.nio.file.*;

public class SncDump extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        File out = new File(args[0]);
        out.mkdirs();
        if (args.length > 1) {
            SymbolTable st = currentProgram.getSymbolTable();
            for (String line : Files.readAllLines(Paths.get(args[1]))) {
                line = line.replaceAll("#.*", "").trim();
                if (line.isEmpty()) continue;
                String[] p = line.split("\\s+");
                Address a = toAddr(Long.decode(p[0]));
                try {
                    if (p.length > 2 && p[2].equals("f")) {
                        Function f = getFunctionAt(a);
                        if (f == null) { disassemble(a); f = createFunction(a, p[1]); }
                        if (f != null) f.setName(p[1], SourceType.USER_DEFINED);
                    } else st.createLabel(a, p[1], SourceType.USER_DEFINED).setPrimary();
                } catch (Exception e) { println("label " + p[1] + ": " + e); }
            }
        }
        Listing listing = currentProgram.getListing();
        try (PrintWriter fl = new PrintWriter(new File(out, "functions.txt"))) {
            for (Function f : listing.getFunctions(true))
                fl.printf("%s %s %d%n", f.getEntryPoint(), f.getName(), f.getBody().getNumAddresses());
        }
        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);
        try (PrintWriter dc = new PrintWriter(new File(out, "decomp.c"))) {
            for (Function f : listing.getFunctions(true)) {
                DecompileResults r = di.decompileFunction(f, 60, monitor);
                dc.printf("// ==== %s @ %s ====%n", f.getName(), f.getEntryPoint());
                if (r != null && r.decompileCompleted()) dc.println(r.getDecompiledFunction().getC());
                else dc.println("// decompile failed");
            }
        }
        try (PrintWriter ls = new PrintWriter(new File(out, "listing.asm"))) {
            for (CodeUnit cu : listing.getCodeUnits(true)) {
                Address a = cu.getAddress();
                Symbol s = currentProgram.getSymbolTable().getPrimarySymbol(a);
                if (s != null) ls.printf("%n%s:%n", s.getName());
                String c = cu.getComment(CodeUnit.EOL_COMMENT);
                if (cu instanceof Instruction) {
                    ls.printf("  %s  %-40s%s%n", a, cu.toString(), c == null ? "" : " ; " + c);
                } else if (cu instanceof Data && ((Data) cu).isDefined()) {
                    ls.printf("  %s  %s%n", a, cu.toString());
                } else {
                    try { ls.printf("  %s  db %02x%n", a, cu.getByte(0) & 0xff); } catch (Exception e) { }
                }
            }
        }
    }
}
