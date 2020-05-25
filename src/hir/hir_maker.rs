use crate::ast::*;
use crate::code_gen::CodeGen;
use crate::error::Error;
use crate::hir;
use crate::hir::*;
use crate::hir::hir_maker_context::*;
use crate::hir::class_dict::ClassDict;
use crate::hir::method_dict::MethodDict;
use crate::names;
use crate::type_checking;

#[derive(Debug, PartialEq)]
pub struct HirMaker {
    pub (in super) class_dict: ClassDict,
    /// List of constants found so far
    pub (in super) constants: HashMap<ConstFullname, TermTy>,
    pub (in super) const_inits: Vec<HirExpression>,
    /// List of string literals found so far
    pub (in super) str_literals: Vec<String>,
}

pub fn make_hir(ast: ast::Program, corelib: Corelib) -> Result<Hir, Error> {
    let class_dict = class_dict::create(&ast, corelib.sk_classes)?;
    let mut hir = convert_program(class_dict, ast)?;

    // While corelib classes are included in `class_dict`,
    // corelib methods are not. Here we need to add them manually
    hir.add_methods(corelib.sk_methods);

    Ok(hir)
}

fn convert_program(class_dict: ClassDict, prog: ast::Program) -> Result<Hir, Error> {
    let mut hir_maker = HirMaker::new(class_dict);
    hir_maker.register_class_consts();
    let main_exprs =
        hir_maker.convert_exprs(&mut HirMakerContext::toplevel(), &prog.exprs)?;
    let method_dict =
        hir_maker.convert_toplevel_defs(&prog.toplevel_defs)?;
    Ok(hir_maker.extract_hir(method_dict, main_exprs))
}

impl HirMaker {
    fn new(class_dict: ClassDict) -> HirMaker {
        HirMaker {
            class_dict,
            constants: HashMap::new(),
            const_inits: vec![],
            str_literals: vec![],
        }
    }

    /// Destructively convert self to Hir
    fn extract_hir(&mut self,
           method_dict: MethodDict,
           main_exprs: HirExpressions) -> Hir {
        // Extract data from self
        let sk_classes = std::mem::replace(&mut self.class_dict.sk_classes, HashMap::new());
        let mut constants = HashMap::new();
        std::mem::swap(&mut constants, &mut self.constants);
        let mut str_literals = vec![];
        std::mem::swap(&mut str_literals, &mut self.str_literals);
        let mut const_inits = vec![];
        std::mem::swap(&mut const_inits, &mut self.const_inits);

        // Register void
        constants.insert(const_fullname("::void"), ty::raw("Void"));

        Hir {
            sk_classes,
            sk_methods: method_dict.sk_methods,
            constants,
            str_literals,
            const_inits,
            main_exprs,
        }
    }

    fn register_class_consts(&mut self) {
        // mem::take is needed to avoid compile error
        let classes = std::mem::take(&mut self.class_dict.sk_classes);
        for name in classes.keys() {
            if !name.is_meta() {
                self.register_class_const(&name);
            }
        }
        self.class_dict.sk_classes = classes;
    }

    /// Register a constant that holds a class
    fn register_class_const(&mut self, fullname: &ClassFullname) {
        let instance_ty = ty::raw(&fullname.0);
        let class_ty = instance_ty.meta_ty();
        let const_name = const_fullname(&format!("::{}", &fullname.0));

        // eg. Constant `A` holds the class A
        self.constants.insert(const_name.clone(), class_ty);
        // eg. "A"
        let idx = self.register_string_literal(&fullname.0);
        // eg. A = Meta:A.new
        let op = Hir::assign_const(const_name, Hir::class_literal(fullname.clone(), idx));
        self.const_inits.push(op);
    }

    fn convert_toplevel_defs(&mut self, toplevel_defs: &[ast::Definition])
                            -> Result<MethodDict, Error> {
        let mut method_dict = MethodDict::new();
        let mut ctx = HirMakerContext::toplevel();

        toplevel_defs.iter().try_for_each(|def|
            match def {
                // Extract instance/class methods
                ast::Definition::ClassDefinition { name, defs, .. } => {
                    let full = name.add_namespace("");
                    self.collect_sk_methods(&full, defs, &mut method_dict)?;
                    Ok(())
                },
                ast::Definition::ConstDefinition { name, expr } => {
                    self.register_const(&mut ctx, name, expr)?;
                    Ok(())
                }
                _ => panic!("should be checked in hir::class_dict")
            }
        )?;

        Ok(method_dict)
    }

    /// Extract instance/class methods and constants
    fn collect_sk_methods(&mut self,
                          fullname: &ClassFullname,
                          defs: &[ast::Definition],
                          method_dict: &mut MethodDict)
                         -> Result<(), Error> {
        self.register_meta_ivar(&fullname)?;
        self.process_defs(defs, method_dict, &fullname)?;
        Ok(())
    }

    fn register_meta_ivar(&mut self, name: &ClassFullname) -> Result<(), Error> {
        let mut meta_ivars = HashMap::new();
        meta_ivars.insert("@name".to_string(), SkIVar {
            name: "@name".to_string(),
            idx: 0,
            ty: ty::raw("String"),
            readonly: true,
        });
        self.class_dict.define_ivars(&name.meta_name(), meta_ivars)?;
        Ok(())
    }

    /// Process each method def and const def
    fn process_defs(&mut self,
                    defs: &[ast::Definition],
                    mut method_dict: &mut MethodDict,
                    fullname: &ClassFullname)
                   -> Result<(), Error> {
        let meta_name = fullname.meta_name();
        let mut ctx = HirMakerContext::class_ctx(&fullname);

        // Add `#initialize`
        let mut own_ivars = HashMap::default();
        if let Some(ast::Definition::InstanceMethodDefinition { sig, body_exprs, .. }) = defs.iter().find(|d| d.is_initializer()) {
            let (sk_method, found_ivars) = self.create_initialize(&mut ctx, &fullname, &sig.name, &body_exprs)?;
            method_dict.add_method(&fullname, sk_method);
            own_ivars = found_ivars;
        }
        self.class_dict.define_ivars(fullname, own_ivars)?;

        // Add `.new`
        if has_new(&fullname) {
            method_dict.add_method(&meta_name, self.create_new(&fullname)?);
        }

        for def in defs.iter().filter(|d| !d.is_initializer()) {
            match def {
                ast::Definition::InstanceMethodDefinition { sig, body_exprs, .. } => {
                    method_dict.add_method(&fullname, 
                                           self.convert_method_def(&ctx, &fullname, &sig.name, &body_exprs)?);
                },
                ast::Definition::ClassMethodDefinition { sig, body_exprs, .. } => {
                    method_dict.add_method(&meta_name,
                                           self.convert_method_def(&ctx, &meta_name, &sig.name, &body_exprs)?);
                },
                ast::Definition::ConstDefinition { name, expr } => {
                    self.register_const(&mut ctx, name, expr)?;
                },
                ast::Definition::ClassDefinition { name, defs, .. } => {
                    let full = name.add_namespace(&fullname.0);
                    self.collect_sk_methods(&full, defs, &mut method_dict)?;
                },
            }
        }
        Ok(())
    }

    /// Create the `initialize` method
    /// Also, define ivars
    fn create_initialize(&mut self,
                         ctx: &mut HirMakerContext,
                         class_fullname: &ClassFullname,
                         name: &MethodFirstname,
                         body_exprs: &[AstExpression]) -> Result<(SkMethod, SkIVars), Error> {
        let super_ivars = self.class_dict.get_superclass(class_fullname)
            .map(|super_cls| super_cls.ivars.clone());
        self.convert_method_def_(ctx, class_fullname, name, body_exprs, true, super_ivars)
    }

    /// Create .new
    fn create_new(&self, fullname: &ClassFullname) -> Result<SkMethod, Error> {
        let class_fullname = fullname.clone();
        let (initialize_name, initialize_params, init_cls_name) = self.find_initialize(&fullname)?;
        let instance_ty = ty::raw(&class_fullname.0);
        let meta_name = class_fullname.meta_name();
        let need_bitcast = init_cls_name != *fullname;
        let arity = initialize_params.len();

        let new_body = move |code_gen: &CodeGen, function: &inkwell::values::FunctionValue| {
            // Allocate memory 
            let obj = code_gen.allocate_sk_obj(&class_fullname, "addr");

            // Call initialize
            let initialize = code_gen.module.get_function(&initialize_name.full_name)
                .unwrap_or_else(|| panic!("[BUG] function `{}' not found", &initialize_name));
            let mut addr = obj;
            if need_bitcast {
                let ances_type = code_gen.llvm_struct_types.get(&init_cls_name)
                    .expect("ances_type not found")
                    .ptr_type(inkwell::AddressSpace::Generic);
                addr = code_gen.builder.build_bitcast(addr, ances_type, "obj_as_super");
            }
            let args = (0..=arity).map(|i| {
                if i == 0 { addr }
                else { function.get_params()[i] }
            }).collect::<Vec<_>>();
            code_gen.builder.build_call(initialize, &args, "");

            code_gen.builder.build_return(Some(&obj));
            Ok(())
        };

        Ok(SkMethod {
            signature: hir::signature_of_new(&meta_name, initialize_params.clone(), &instance_ty),
            body: SkMethodBody::RustClosureMethodBody {
                boxed_gen: Box::new(new_body),
            }
        })
    }

    fn find_initialize(&self, class_fullname: &ClassFullname)
                       -> Result<(MethodFullname, &Vec<MethodParam>, ClassFullname), Error> {
        let (sig, found_cls) =
            self.class_dict.lookup_method(&class_fullname,
                                          &method_firstname("initialize"))?;
        Ok((names::method_fullname(&found_cls, "initialize"), &sig.params, found_cls))
    }

    /// Register a constant
    pub (in super) fn register_const(&mut self,
                      ctx: &mut HirMakerContext,
                      name: &ConstFirstname,
                      expr: &AstExpression) -> Result<ConstFullname, Error> {
        // TODO: resolve name using ctx
        let fullname = const_fullname(&format!("{}::{}", ctx.namespace.0, &name.0));
        let hir_expr = self.convert_expr(ctx, expr)?;
        self.constants.insert(fullname.clone(), hir_expr.ty.clone());
        let op = Hir::assign_const(fullname.clone(), hir_expr);
        self.const_inits.push(op);
        Ok(fullname)
    }

    fn convert_method_def(&mut self,
                          ctx: &HirMakerContext,
                          class_fullname: &ClassFullname,
                          name: &MethodFirstname,
                          body_exprs: &[AstExpression]) -> Result<SkMethod, Error> {
        let (sk_method, _ivars) =
            self.convert_method_def_(ctx, class_fullname, name, body_exprs, false, None)?;
        Ok(sk_method)
    }

    /// Create a SkMethod and return it with ctx.iivars
    fn convert_method_def_(&mut self,
                          ctx: &HirMakerContext,
                          class_fullname: &ClassFullname,
                          name: &MethodFirstname,
                          body_exprs: &[AstExpression],
                          is_initializer: bool,
                          super_ivars: Option<SkIVars>)
                          -> Result<(SkMethod, HashMap<String, SkIVar>), Error> {
        // MethodSignature is built beforehand by class_dict::new
        let err = format!("[BUG] signature not found ({}/{}/{:?})", class_fullname, name, self.class_dict);
        let signature = self.class_dict.find_method(class_fullname, name).expect(&err).clone();

        let mut method_ctx = HirMakerContext::method_ctx(ctx, &signature, is_initializer);
        if let Some(x) = super_ivars {
            method_ctx.super_ivars = x;
        }

        let body_exprs = self.convert_exprs(&mut method_ctx, body_exprs)?;
        type_checking::check_return_value(&signature, &body_exprs.ty)?;

        let body = SkMethodBody::ShiikaMethodBody { exprs: body_exprs };

        Ok((SkMethod { signature, body }, method_ctx.iivars))
    }
}

// Whether the class has .new
fn has_new(fullname: &ClassFullname) -> bool {
    // TODO: maybe more?
    // At least these two must be excluded (otherwise wrong .ll is generated)
    if fullname.0 == "Int" || fullname.0 == "Float" {
        return false
    }
    true
}
